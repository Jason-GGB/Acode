import { expect, it } from "vitest";
import { projectSession } from "@/services/chat/sessionProjection";
import type { ChatMessage, ToolActivity } from "@/types/chat";

function message(partial: Pick<ChatMessage, "id" | "role" | "content" | "status">): ChatMessage {
  return {
    sessionId: "s",
    timestamp: 1,
    ...partial,
  };
}

it("folds guidance into the assistant timeline on the workbench", () => {
  const projection = projectSession({
    messages: [
      message({ id: "u", role: "user", content: "fix the test", status: "done" }),
      message({ id: "a", role: "assistant", content: "looking", status: "streaming" }),
      message({
        id: "g",
        role: "user",
        content: "<!--peek:soft-inject-->\nfocus on the projection",
        status: "done",
      }),
    ],
    sending: true,
  });
  expect(projection.turns.map((turn) => turn.user?.id ?? null)).toEqual(["u"]);
  expect(projection.turns[0]?.body.map((item) => item.kind)).toEqual(["assistant"]);
  expect(projection.turns[0]?.body[0]?.message.workTimeline).toEqual([
    { type: "inject", id: "g", content: "focus on the projection" },
  ]);
});

it("keeps guidance as its own row when the overlay is inline", () => {
  const projection = projectSession({
    messages: [
      message({ id: "u", role: "user", content: "fix the test", status: "done" }),
      message({ id: "a", role: "assistant", content: "looking", status: "streaming" }),
      {
        ...message({ id: "g", role: "user", content: "focus on the projection", status: "done" }),
        injected: true,
      },
    ],
    sending: true,
    inline: true,
  });
  expect(projection.turns).toHaveLength(1);
  expect(projection.turns[0]?.body.map((item) => [item.kind, item.message.id])).toEqual([
    ["assistant", "a"],
    ["inject", "g"],
  ]);
  expect(projection.turns[0]?.body[0]?.message.workTimeline).toBeUndefined();
});

it("shows the assistant reply when the timeline has not recorded it yet", () => {
  const projection = projectSession({
    messages: [message({ id: "a", role: "assistant", content: "Final answer", status: "done" })],
    showReasoning: true,
    displayMode: "detailed",
  });
  expect(projection.turns[0]?.body[0]?.timeline).toEqual([
    { type: "content", id: "content-final-0", content: "Final answer" },
  ]);
});

it("omits inline guidance from the assistant timeline", () => {
  const projection = projectSession({
    messages: [
      {
        ...message({ id: "a", role: "assistant", content: "", status: "streaming" }),
        workTimeline: [{ type: "inject", id: "guidance-1", content: "Follow-up" }],
      },
    ],
    inline: true,
    showReasoning: true,
    displayMode: "detailed",
  });
  expect(projection.turns[0]?.body[0]?.timeline).toEqual([]);
});

it("folds a shell command into process details when the display is compact", () => {
  const projection = projectSession({
    messages: [
      {
        ...message({ id: "a", role: "assistant", content: "ran it", status: "done" }),
        toolActivities: [
          activity({ id: "sh", toolName: "run_shell", status: "done", kind: "shell" }),
        ],
        workTimeline: [{ type: "tool", id: "t", toolActivityId: "sh" }],
      },
    ],
    displayMode: "compact",
    showReasoning: false,
  });
  const timeline = projection.turns[0]?.body[0]?.timeline ?? [];
  expect(timeline.some((segment) => segment.type === "process")).toBe(true);
  expect(timeline.some((segment) => segment.type === "inline")).toBe(false);
});

it("hides provider-setup text from the timeline", () => {
  const projection = projectSession({
    messages: [
      message({ id: "a", role: "assistant", content: "No model selected", status: "error" }),
    ],
    suppressContentIds: ["a"],
    showReasoning: true,
  });
  const timeline = projection.turns[0]?.body[0]?.timeline ?? [];
  expect(timeline.some((segment) => segment.type === "content")).toBe(false);
});

it("treats a sending flag as the session still outputting", () => {
  const projection = projectSession({
    messages: [message({ id: "u", role: "user", content: "hi", status: "done" })],
    sending: true,
  });
  expect(projection.outputting).toBe(true);
  expect(projection.outputtingMessageId).toBe("");
});

it("names the assistant message that is still streaming", () => {
  const projection = projectSession({
    messages: [
      message({ id: "u", role: "user", content: "hi", status: "done" }),
      message({ id: "a1", role: "assistant", content: "done", status: "done" }),
      message({ id: "a2", role: "assistant", content: "working", status: "streaming" }),
    ],
    sending: false,
  });
  expect(projection.outputting).toBe(true);
  expect(projection.outputtingMessageId).toBe("a2");
});

it("finishes when nothing is sending and the assistant is done", () => {
  const projection = projectSession({
    messages: [message({ id: "a", role: "assistant", content: "done", status: "done" })],
    sending: false,
  });
  expect(projection.outputting).toBe(false);
  expect(projection.outputtingMessageId).toBe("");
});

function activity(
  partial: Pick<ToolActivity, "id" | "toolName" | "status"> & { kind?: string },
): ToolActivity {
  return {
    title: partial.toolName,
    kind: "delegate",
    success: partial.status === "done",
    ...partial,
  };
}

it("treats a running subagent entry as still outputting", () => {
  const projection = projectSession({
    messages: [message({ id: "child", role: "assistant", content: "waiting", status: "done" })],
    sending: false,
    parentActivities: [activity({ id: "run-1", toolName: "run_subagent", status: "running" })],
    subagentEntryId: "run-1:0",
    language: "en-US",
  });
  expect(projection.outputting).toBe(true);
  expect(projection.outputtingMessageId).toBe("");
});

it("does not keep a finished subagent entry outputting", () => {
  const projection = projectSession({
    messages: [message({ id: "child", role: "assistant", content: "done", status: "done" })],
    sending: false,
    parentActivities: [activity({ id: "run-1", toolName: "run_subagent", status: "done" })],
    subagentEntryId: "run-1:0",
    language: "en-US",
  });
  expect(projection.outputting).toBe(false);
});
