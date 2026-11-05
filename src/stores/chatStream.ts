/**
 * Stream / work-timeline helpers for the chat Pinia store.
 */

import {
  appendTimelineInject,
  timelineHasInject,
  withTimelineInject,
} from "@/services/chat/timelineInject";
import type { ChatMessage, WorkTimelineItem } from "@/types/chat";

export { appendTimelineInject, timelineHasInject, withTimelineInject };

/** Messages updated through the live append path already have matching text and timeline. */
const synchronizedLiveTimelines = new WeakSet<ChatMessage>();
const unsynchronizedLiveTimelines = new WeakSet<ChatMessage>();

export function hasSynchronizedLiveTimeline(message: ChatMessage): boolean {
  return synchronizedLiveTimelines.has(message);
}

function timelineMatchesMessage(message: ChatMessage): boolean {
  const timeline = message.workTimeline ?? [];
  let content = "";
  let reasoning = "";
  for (const item of timeline) {
    if (item.type === "content") content += item.content;
    if (item.type === "reasoning") reasoning += item.content;
  }
  return content === message.content && reasoning === (message.reasoning ?? "");
}

/** Live messages are owned by the reactive store. Keep completed rows and timeline
 * segments stable while appending only the active tail; immutable helpers below
 * remain available for history/retry snapshots. */
export function applyLiveTextDelta(message: ChatMessage, content: string, reasoning: string) {
  if (!synchronizedLiveTimelines.has(message) && !unsynchronizedLiveTimelines.has(message)) {
    if (timelineMatchesMessage(message)) synchronizedLiveTimelines.add(message);
    else unsynchronizedLiveTimelines.add(message);
  }
  const append = (chunk: string, kind: "reasoning" | "content") => {
    if (!chunk) return;
    const timeline = message.workTimeline ?? (message.workTimeline = []);
    const last = timeline[timeline.length - 1];
    if (last?.type === kind) last.content += chunk;
    else
      timeline.push({ type: kind, id: `${kind}-${Date.now()}-${timeline.length}`, content: chunk });
  };
  append(reasoning, "reasoning");
  append(content, "content");
  if (content) message.content += content;
  if (reasoning) message.reasoning = (message.reasoning ?? "") + reasoning;
  if (content || reasoning) message.status = "streaming";
}

/** Restore the backend's committed prefix rather than erasing completed work. */
export function rollbackStreamAttempt(current: ChatMessage, snapshot?: ChatMessage): ChatMessage {
  const valid = snapshot?.id === current.id && snapshot.sessionId === current.sessionId;
  const keptActivities = current.toolActivities?.filter(
    (activity) => activity.status !== "running",
  );
  return {
    ...current,
    content: valid ? snapshot.content : "",
    reasoning: valid ? snapshot.reasoning : undefined,
    workTimeline: valid ? snapshot.workTimeline?.map((item) => ({ ...item })) : undefined,
    toolActivities: valid
      ? snapshot.toolActivities
      : keptActivities?.length
        ? keptActivities
        : undefined,
  };
}

/**
 * Append a text chunk (reasoning or regular content) to the work timeline,
 * merging into the trailing segment when it's the same kind so consecutive
 * deltas don't fragment into one segment per network chunk. Tool activities
 * are inserted separately (see `upsertToolActivity`), so a new segment only
 * starts here once a tool call has broken the run of same-kind text.
 */
export function appendTimelineText(
  timeline: WorkTimelineItem[] | undefined,
  chunk: string,
  kind: "reasoning" | "content",
): WorkTimelineItem[] {
  const next = [...(timeline ?? [])];
  const last = next[next.length - 1];
  if (last?.type === kind) {
    next[next.length - 1] = { ...last, content: last.content + chunk };
  } else {
    next.push({
      type: kind,
      id: `${kind}-${Date.now()}-${next.length}`,
      content: chunk,
    });
  }
  return next;
}

export function findLastMessageIndex(
  messages: ChatMessage[],
  predicate: (message: ChatMessage) => boolean,
) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (predicate(messages[index])) return index;
  }
  return -1;
}
