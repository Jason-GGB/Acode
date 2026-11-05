import { askUserAnswersForActivity, isAskUserTool } from "@/services/chat/askUserAnswer";
import { shouldHidePlanChromeActivity } from "@/services/chat/planProposal";
import { isImageGenActivity } from "@/services/chat/toolActivityEnrichment";
import type { AskUserAnswerItem, ChatMessage, ToolActivity } from "@/types/chat";
import type { AgentWorkDisplay } from "@/types/setting";

export type WorkSegment =
  | { type: "reasoning"; id: string; content: string }
  | { type: "narration"; id: string; content: string }
  | { type: "content"; id: string; content: string }
  | { type: "inject"; id: string; content: string }
  | { type: "ask-answer"; id: string; items: AskUserAnswerItem[] }
  | { type: "inline"; id: string; activities: ToolActivity[]; operations: boolean }
  | { type: "process"; id: string; activities: ToolActivity[]; operations: boolean };

const SHOWCASE_KINDS = new Set(["shell", "create", "edit", "delete", "move", "image"]);
const TASK_LIST_TOOLS = new Set(["update_tasks", "todo_write"]);

export type MessageTimelineInput = {
  inline?: boolean;
  showReasoning?: boolean;
  displayMode?: AgentWorkDisplay;
  suppressContent?: boolean;
  liveTimelineSynchronized?: boolean;
};

/** 把一条助手消息对齐成用户看得见的 timeline。 */
export function projectMessageTimeline(
  message: ChatMessage,
  options: MessageTimelineInput = {},
): WorkSegment[] {
  const displayMode = options.displayMode ?? "detailed";
  const streaming = message.status === "pending" || message.status === "streaming";
  const recorded = (message.toolActivities ?? []).filter((activity) => !activity.parentActivityId);
  const activityById = new Map(recorded.map((activity) => [activity.id, activity]));
  let out: WorkSegment[] = [];
  const seen = new Set<string>();

  for (const item of message.workTimeline ?? []) {
    if (item.type === "content" && options.suppressContent) continue;
    if (item.type === "inject") {
      if (options.inline) continue;
      if (item.content.trim()) {
        out.push({ type: "inject", id: item.id, content: item.content });
      }
      continue;
    }
    if (item.type === "reasoning" || item.type === "content") {
      if (item.content.trim()) out.push({ ...item });
      continue;
    }
    const activity = activityById.get(item.toolActivityId);
    if (!activity) continue;
    absorbActivity(out, seen, activity, message, displayMode);
  }

  const synchronized = options.liveTimelineSynchronized === true;
  if (!streaming || !synchronized) {
    reconcileTrailingText(out, "reasoning", message.reasoning);
    if (!options.suppressContent) reconcileTrailingText(out, "content", message.content);
  }

  if (options.showReasoning === false) {
    out = out.filter((segment) => segment.type !== "reasoning");
  }

  for (const activity of recorded) {
    absorbActivity(out, seen, activity, message, displayMode);
  }
  return coalesceCompletedNarration(
    coalesceCompletedReasoning(out, message, streaming, options.showReasoning),
    message,
    streaming,
  );
}

function absorbActivity(
  out: WorkSegment[],
  seen: Set<string>,
  activity: ToolActivity,
  message: ChatMessage,
  displayMode: AgentWorkDisplay,
) {
  if (shouldHidePlanChromeActivity(activity, message)) {
    seen.add(activity.id);
    return;
  }
  if (isAskUserTool(activity.toolName)) {
    considerAskUser(out, seen, activity, message, displayMode);
    return;
  }
  considerActivity(out, seen, activity, displayMode);
}

function isInlineActivity(activity: ToolActivity, displayMode: AgentWorkDisplay): boolean {
  if (isImageGenActivity(activity)) return activity.status !== "running";
  if (TASK_LIST_TOOLS.has(activity.toolName)) return true;
  if (displayMode === "compact") return false;
  return SHOWCASE_KINDS.has(activity.kind);
}

function isOperationsActivity(activity: ToolActivity): boolean {
  return (
    SHOWCASE_KINDS.has(activity.kind) && activity.kind !== "shell" && activity.kind !== "image"
  );
}

function segmentKind(activity: ToolActivity, displayMode: AgentWorkDisplay): "inline" | "process" {
  return isInlineActivity(activity, displayMode) ? "inline" : "process";
}

function canMergeActivities(last: ToolActivity, next: ToolActivity): boolean {
  if (isImageGenActivity(last) || isImageGenActivity(next)) {
    return isImageGenActivity(last) && isImageGenActivity(next);
  }
  return isOperationsActivity(last) === isOperationsActivity(next);
}

function pushActivity(
  segments: WorkSegment[],
  activity: ToolActivity,
  displayMode: AgentWorkDisplay,
) {
  const kind = segmentKind(activity, displayMode);
  const operations = isOperationsActivity(activity);
  const last = segments[segments.length - 1];
  if (
    last &&
    last.type === kind &&
    last.operations === operations &&
    last.activities[0] &&
    canMergeActivities(last.activities[0], activity)
  ) {
    if (!last.activities.some((item) => item.id === activity.id)) last.activities.push(activity);
    return;
  }
  const base = { id: `${kind}-${activity.id}`, activities: [activity], operations };
  segments.push(kind === "inline" ? { type: "inline", ...base } : { type: "process", ...base });
}

function considerAskUser(
  segments: WorkSegment[],
  seen: Set<string>,
  activity: ToolActivity,
  message: ChatMessage,
  displayMode: AgentWorkDisplay,
) {
  if (seen.has(activity.id)) return;
  seen.add(activity.id);
  const items = askUserAnswersForActivity(activity, message.askUserAnswer);
  if (items.length > 0) {
    segments.push({ type: "ask-answer", id: `ask-answer-${activity.id}`, items });
    return;
  }
  if (activity.status === "running") pushActivity(segments, activity, displayMode);
}

function parallelSubagentFingerprint(activity: ToolActivity): string | undefined {
  if (activity.toolName !== "run_parallel_subagents") return undefined;
  const tasks = activity.arguments?.tasks;
  if (!Array.isArray(tasks)) return activity.id;
  const prompts = tasks.map((value) => {
    if (typeof value !== "object" || value == null) return "";
    return String((value as Record<string, unknown>).prompt ?? "").trim();
  });
  return `${activity.toolName}:${prompts.join("\0")}`;
}

function considerActivity(
  segments: WorkSegment[],
  seen: Set<string>,
  activity: ToolActivity,
  displayMode: AgentWorkDisplay,
) {
  if (seen.has(activity.id)) return;
  seen.add(activity.id);
  const fingerprint = parallelSubagentFingerprint(activity);
  if (fingerprint) {
    for (const segment of segments) {
      if (segment.type !== "process" && segment.type !== "inline") continue;
      const index = segment.activities.findIndex(
        (item) => parallelSubagentFingerprint(item) === fingerprint,
      );
      if (index !== -1) {
        segment.activities[index] = activity;
        return;
      }
    }
  }
  pushActivity(segments, activity, displayMode);
}

type TextSegment = Extract<WorkSegment, { type: "reasoning" | "content" }>;

function isTextSegment(segment: WorkSegment): segment is TextSegment {
  return segment.type === "reasoning" || segment.type === "content";
}

function reconcileTrailingText(
  out: WorkSegment[],
  kind: "reasoning" | "content",
  finalText: string | undefined,
) {
  const finalValue = finalText ?? "";
  if (!finalValue) return;
  let accumulated = "";
  for (const segment of out) {
    if (isTextSegment(segment) && segment.type === kind) accumulated += segment.content;
  }
  if (finalValue === accumulated) return;

  if (!accumulated || finalValue.startsWith(accumulated)) {
    const missing = finalValue.slice(accumulated.length);
    if (!missing) return;
    const last = out[out.length - 1];
    if (last && isTextSegment(last) && last.type === kind) {
      last.content += missing;
    } else {
      out.push({ type: kind, id: `${kind}-final-${out.length}`, content: missing });
    }
    return;
  }

  if (accumulated.includes(finalValue)) return;
  const notice = finalValue.trim();
  if (!notice || accumulated.includes(notice)) return;
  out.push({
    type: kind,
    id: `${kind}-final-${out.length}`,
    content: notice.startsWith("已停止") || notice.startsWith("Stopped") ? `\n\n${notice}` : notice,
  });
}

function coalesceCompletedReasoning(
  out: WorkSegment[],
  message: ChatMessage,
  streaming: boolean,
  showReasoning: boolean | undefined,
): WorkSegment[] {
  if (streaming) return out;
  const fromMessage = message.reasoning?.trim() ?? "";
  const fromSegments = out
    .filter((segment): segment is Extract<WorkSegment, { type: "reasoning" }> => {
      return segment.type === "reasoning";
    })
    .map((segment) => segment.content)
    .join("");
  const combined = pickFullText(fromMessage, fromSegments);
  const withoutReasoning = out.filter((segment) => segment.type !== "reasoning");
  if (!combined || showReasoning === false) return withoutReasoning;

  const firstIdx = out.findIndex((segment) => segment.type === "reasoning");
  const block: WorkSegment = {
    type: "reasoning",
    id: `${message.id}-reasoning-completed`,
    content: combined,
  };
  if (firstIdx <= 0) return [block, ...withoutReasoning];
  const before = out.slice(0, firstIdx).filter((segment) => segment.type !== "reasoning");
  const after = out.slice(firstIdx).filter((segment) => segment.type !== "reasoning");
  return [...before, block, ...after];
}

function pickFullText(fromMessage: string, fromSegments: string): string {
  if (!fromMessage) return fromSegments;
  if (!fromSegments) return fromMessage;
  if (fromMessage === fromSegments) return fromMessage;
  if (fromMessage.startsWith(fromSegments)) return fromMessage;
  if (fromSegments.startsWith(fromMessage)) return fromSegments;
  return fromSegments.length >= fromMessage.length ? fromSegments : fromMessage;
}

function coalesceCompletedNarration(
  out: WorkSegment[],
  message: ChatMessage,
  streaming: boolean,
): WorkSegment[] {
  if (streaming) return out;
  const hasTools = out.some(
    (segment) =>
      segment.type === "process" || segment.type === "inline" || segment.type === "ask-answer",
  );
  if (!hasTools) return out;

  let lastToolIdx = -1;
  for (let i = 0; i < out.length; i++) {
    if (out[i].type === "process" || out[i].type === "inline" || out[i].type === "ask-answer") {
      lastToolIdx = i;
    }
  }
  if (lastToolIdx < 0) return out;

  const intermediate: string[] = [];
  const kept: WorkSegment[] = [];
  let insertAt = 0;
  for (let i = 0; i < out.length; i++) {
    const segment = out[i];
    if (segment.type === "content" && i < lastToolIdx) {
      if (segment.content.trim()) intermediate.push(segment.content.trim());
      if (intermediate.length === 1) insertAt = kept.length;
      continue;
    }
    kept.push(segment);
  }

  const joined = intermediate.join("\n\n").trim();
  const hasVisibleReply = kept.some((segment) => segment.type === "content");
  let visibleTail = "";
  let folded = joined;
  if (!hasVisibleReply && intermediate.length > 0) {
    visibleTail = intermediate[intermediate.length - 1] ?? "";
    folded = intermediate.slice(0, -1).join("\n\n").trim();
  }
  if (folded) {
    if (kept[0]?.type === "reasoning") insertAt = Math.max(insertAt, 1);
    kept.splice(insertAt, 0, {
      type: "narration",
      id: `${message.id}-narration-completed`,
      content: folded,
    });
  }
  if (visibleTail) {
    kept.push({ type: "content", id: `${message.id}-narration-tail`, content: visibleTail });
  }
  return kept;
}
