import { findSubagentEntry } from "@/services/chat/subagentPanel";
import { isCompactionSummary } from "@/services/chat/compactMarker";
import { isSoftInjectContent } from "@/services/chat/softInject";
import { normalizeRole } from "@/services/chat/normalize";
import { projectMessageTimeline, type WorkSegment } from "@/services/chat/sessionTimeline";
import type { ChatMessage, ToolActivity } from "@/types/chat";
import { withTimelineInject } from "@/services/chat/timelineInject";
import type { AgentWorkDisplay, AppLanguage } from "@/types/setting";

export type VisibleTurnBody = {
  kind: "assistant" | "inject";
  message: ChatMessage;
  timeline?: WorkSegment[];
};

export type VisibleTurn = {
  key: string;
  user: ChatMessage | null;
  body: VisibleTurnBody[];
};

export type SessionProjection = {
  turns: VisibleTurn[];
  outputting: boolean;
  outputtingMessageId: string;
};

export type ProjectSessionInput = {
  messages: ChatMessage[];
  sending?: boolean;
  inline?: boolean;
  showReasoning?: boolean;
  displayMode?: AgentWorkDisplay;
  suppressContentIds?: readonly string[];
  synchronizedMessageIds?: ReadonlySet<string>;
  parentActivities?: ToolActivity[];
  subagentEntryId?: string;
  language?: AppLanguage;
};

/** 按一条 session 交出可见 turn，以及它是否还在输出。 */
export function projectSession(input: ProjectSessionInput): SessionProjection {
  const outputtingMessageId = liveAssistantId(input.messages);
  return {
    turns: attachTimelines(groupTurns(input.messages, Boolean(input.inline)), input),
    outputting:
      Boolean(input.sending) || Boolean(outputtingMessageId) || subagentEntryRunning(input),
    outputtingMessageId,
  };
}

function attachTimelines(turns: VisibleTurn[], input: ProjectSessionInput): VisibleTurn[] {
  const suppress = new Set(input.suppressContentIds ?? []);
  return turns.map((turn) => ({
    ...turn,
    body: turn.body.map((item) => {
      if (item.kind !== "assistant") return item;
      return {
        ...item,
        timeline: projectMessageTimeline(item.message, {
          inline: input.inline,
          showReasoning: input.showReasoning,
          displayMode: input.displayMode,
          suppressContent: suppress.has(item.message.id),
          liveTimelineSynchronized: input.synchronizedMessageIds?.has(item.message.id),
        }),
      };
    }),
  }));
}

function liveAssistantId(messages: ChatMessage[]): string {
  return (
    [...messages]
      .reverse()
      .find(
        (message) =>
          normalizeRole(message.role) === "assistant" &&
          (message.status === "pending" || message.status === "streaming"),
      )?.id ?? ""
  );
}

function subagentEntryRunning(input: ProjectSessionInput): boolean {
  if (!input.subagentEntryId || !input.parentActivities) return false;
  const entry = findSubagentEntry(
    input.parentActivities,
    input.subagentEntryId,
    input.language ?? "en-US",
  );
  return entry?.status === "running";
}

type VisibleItem =
  | { kind: "user"; key: string; message: ChatMessage }
  | { kind: "assistant"; key: string; message: ChatMessage }
  | { kind: "inject"; key: string; message: ChatMessage };

function groupTurns(messages: ChatMessage[], inline: boolean): VisibleTurn[] {
  const turns: VisibleTurn[] = [];
  let current: VisibleTurn | null = null;
  for (const item of visibleItems(messages)) {
    if (item.kind === "user") {
      if (current) turns.push(current);
      current = { key: `turn-${item.key}`, user: item.message, body: [] };
      continue;
    }
    if (item.kind === "inject") {
      if (inline && current) {
        current.body.push(item);
        continue;
      }
      if (current) {
        const assistantIndex = lastAssistantIndex(current.body);
        if (assistantIndex !== -1) {
          const assistant = current.body[assistantIndex]!;
          current.body[assistantIndex] = {
            kind: "assistant",
            message: withTimelineInject(assistant.message, item.message),
          };
          continue;
        }
        current.body.push({ kind: "inject", message: item.message });
      } else {
        current = {
          key: `turn-${item.key}`,
          user: null,
          body: [{ kind: "inject", message: item.message }],
        };
      }
      continue;
    }
    if (!current) {
      current = {
        key: `turn-${item.key}`,
        user: null,
        body: [{ kind: "assistant", message: item.message }],
      };
    } else {
      current.body.push({ kind: "assistant", message: item.message });
    }
  }
  if (current) turns.push(current);
  return turns;
}

function lastAssistantIndex(body: VisibleTurnBody[]): number {
  for (let index = body.length - 1; index >= 0; index -= 1) {
    if (body[index]?.kind === "assistant") return index;
  }
  return -1;
}

function visibleItems(messages: ChatMessage[]): VisibleItem[] {
  const items: VisibleItem[] = [];
  for (const message of messages) {
    const role = String(message.role).toLowerCase();
    if (role === "system" || role === "tool" || message.id.startsWith("compact-")) continue;
    if (
      message.status !== "pending" &&
      message.status !== "streaming" &&
      isCompactionSummary(message)
    ) {
      continue;
    }
    if (role === "user") {
      items.push({
        kind: message.injected === true || isSoftInjectContent(message.content) ? "inject" : "user",
        key: message.id,
        message,
      });
      continue;
    }
    items.push({ kind: "assistant", key: message.id, message });
  }
  return items;
}
