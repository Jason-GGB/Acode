import { stripSoftInjectMarker } from "@/services/chat/softInject";
import type { ChatMessage, WorkTimelineItem } from "@/types/chat";

/** Insert a mid-turn follow-up at the current end of the assistant timeline. */
export function appendTimelineInject(
  timeline: WorkTimelineItem[] | undefined,
  id: string,
  content: string,
): WorkTimelineItem[] {
  const text = content.trim();
  if (!text) {
    return [...(timeline ?? [])];
  }
  const next = [...(timeline ?? [])];
  if (
    next.some((item) => item.type === "inject" && (item.id === id || item.content.trim() === text))
  ) {
    return next;
  }
  next.push({ type: "inject", id, content: text });
  return next;
}

/** 时间线里是否已经有这条中途追问。 */
export function timelineHasInject(
  timeline: WorkTimelineItem[] | undefined,
  inject: ChatMessage,
): boolean {
  const text = stripSoftInjectMarker(inject.content).trim();
  return (timeline ?? []).some(
    (item) => item.type === "inject" && (item.id === inject.id || item.content.trim() === text),
  );
}

/** 把中途追问折进助手消息的 timeline；已经折过则原样返回。 */
export function withTimelineInject(assistant: ChatMessage, inject: ChatMessage): ChatMessage {
  if (timelineHasInject(assistant.workTimeline, inject)) {
    return assistant;
  }
  return {
    ...assistant,
    workTimeline: appendTimelineInject(
      assistant.workTimeline,
      inject.id,
      stripSoftInjectMarker(inject.content),
    ),
  };
}
