<template>
  <div
    v-if="segments.length"
    class="agent-work"
    :class="{ 'has-running-subagent': hasRunningSubagent }"
  >
    <!-- Live turn: keep chronological interleaving for follow-along. -->
    <template v-if="streaming">
      <AgentWorkSegment
        v-for="(segment, segmentIndex) in segments"
        :key="segment.id"
        :segment="segment"
        :streaming="streaming"
        :follow="streaming && segment.id === lastSegmentId"
        :language="language"
        :show-reasoning-summary="isFirstReasoningSegment(segmentIndex)"
        :show-narration-summary="true"
        :all-activities="visibleActivities"
        :collapsible="segment.type === 'process' && processSegmentCollapsible(segment)"
        :process-open="segment.type === 'process' && isProcessOpen(segment.id)"
        :headline="segment.type === 'process' ? processHeadline(segment) : ''"
        :cards-collapsed="displayMode === 'compact'"
        @inspect-subagent="emit('inspectSubagent', $event)"
        @preview-image="emit('previewImage', $event)"
        @edit-from-image="emit('editFromImage', $event)"
        @toggle-process="toggleProcess"
      />
    </template>

    <!-- Completed: fold thinking + tools; keep generated images and the final reply open. -->
    <template v-else>
      <details
        v-if="preambleSegments.length"
        class="agent-work-fold"
        :open="foldOpen"
        @toggle="handleFoldToggle"
      >
        <summary class="agent-work-fold-summary" :class="{ pinned: foldOpen }">
          <ChevronRight class="agent-work-fold-chevron" :class="{ open: foldOpen }" :size="12" />
          <span>{{ foldLabel }}</span>
          <span v-if="foldMeta" class="agent-work-fold-meta">{{ foldMeta }}</span>
        </summary>
        <div v-if="foldOpen" class="agent-work-fold-body">
          <AgentWorkSegment
            v-for="segment in preambleSegments"
            :key="segment.id"
            :segment="segment"
            :streaming="false"
            :follow="false"
            :language="language"
            :show-reasoning-summary="false"
            :show-narration-summary="false"
            :all-activities="visibleActivities"
            :collapsible="segment.type === 'process' && processSegmentCollapsible(segment)"
            :process-open="segment.type === 'process' && isProcessOpen(segment.id)"
            :headline="segment.type === 'process' ? processHeadline(segment) : ''"
            :cards-collapsed="displayMode === 'compact'"
            @inspect-subagent="emit('inspectSubagent', $event)"
            @preview-image="emit('previewImage', $event)"
            @edit-from-image="emit('editFromImage', $event)"
            @toggle-process="toggleProcess"
          />
        </div>
      </details>

      <AgentWorkSegment
        v-for="segment in replySegments"
        :key="segment.id"
        :segment="segment"
        :streaming="false"
        :follow="false"
        :language="language"
        :show-reasoning-summary="true"
        :show-narration-summary="true"
        :all-activities="visibleActivities"
        :collapsible="false"
        :process-open="false"
        headline=""
        :cards-collapsed="displayMode === 'compact'"
        @inspect-subagent="emit('inspectSubagent', $event)"
        @preview-image="emit('previewImage', $event)"
        @edit-from-image="emit('editFromImage', $event)"
        @toggle-process="toggleProcess"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ChevronRight } from "@lucide/vue";
import { computed, inject, reactive, ref, watch } from "vue";
import AgentWorkSegment, { type WorkSegment } from "@/components/chat/AgentWorkSegment.vue";
import type { ChatMessage } from "@/types/chat";
import type { AgentWorkDisplay, AppLanguage } from "@/types/setting";
import { askUserAnswersForActivity, isAskUserTool } from "@/services/chat/askUserAnswer";
import { shouldHidePlanChromeActivity } from "@/services/chat/planProposal";
import { SUBAGENT_TOOLS } from "@/services/chat/subagentTools";
import {
  countReasoningSegments,
  isProcessSegmentCollapsible,
  summarizeProcessActivities,
  summarizeWorkFoldMeta,
} from "@/services/chat/toolActivityDisplay";
import { activityMatchesQuery, textIncludesQuery } from "@/services/chat/conversationFind";
import { conversationFindKey } from "@/composables/chat/useConversationFind";
import { isImageGenActivity } from "@/services/chat/toolActivityEnrichment";
import { tr } from "@/services/i18n";

const props = withDefaults(
  defineProps<{
    message: ChatMessage;
    language?: AppLanguage;
    /** detailed = shell/diff inline; compact = fold into process details. */
    displayMode?: AgentWorkDisplay;
    segments?: WorkSegment[];
  }>(),
  {
    displayMode: "detailed",
    segments: () => [],
  },
);
const emit = defineEmits<{
  inspectSubagent: [activityId: string];
  previewImage: [source: string];
  editFromImage: [payload: import("@/services/chat/imageEditReference").ImageEditReferencePayload];
}>();

type TimelineSegment = WorkSegment;

const SHOWCASE_KINDS = new Set(["shell", "create", "edit", "delete", "move", "image"]);

const processOpen = reactive(new Map<string, boolean>());
const userToggledProcess = reactive(new Set<string>());

const foldOpen = ref(false);
const foldPinned = ref(false);

const streaming = computed(
  () => props.message.status === "pending" || props.message.status === "streaming",
);
const waitingForAskUser = computed(
  () =>
    props.message.toolActivities?.some(
      (activity) =>
        isAskUserTool(activity.toolName) &&
        activity.status === "running" &&
        askUserAnswersForActivity(activity, props.message.askUserAnswer).length === 0,
    ) ?? false,
);
const recordedActivities = computed(() =>
  (props.message.toolActivities ?? []).filter((activity) => !activity.parentActivityId),
);
const visibleActivities = computed(() =>
  recordedActivities.value.filter(
    (activity) =>
      !(isAskUserTool(activity.toolName) && activity.status !== "running") &&
      !shouldHidePlanChromeActivity(activity, props.message),
  ),
);

const hasRunningSubagent = computed(() =>
  recordedActivities.value.some(
    (activity) => activity.status === "running" && SUBAGENT_TOOLS.has(activity.toolName),
  ),
);

const segments = computed(() => props.segments ?? []);

function isGeneratedImageSegment(segment: TimelineSegment): boolean {
  return (
    (segment.type === "inline" || segment.type === "process") &&
    segment.activities.length > 0 &&
    segment.activities.every((activity) => isImageGenActivity(activity))
  );
}

/** After the turn completes, images stay next to the reply instead of inside the fold. */
function isReplySegment(segment: TimelineSegment): boolean {
  return (
    segment.type === "content" ||
    segment.type === "inject" ||
    segment.type === "ask-answer" ||
    isGeneratedImageSegment(segment)
  );
}

const preambleSegments = computed(() =>
  segments.value.filter((segment) => !isReplySegment(segment)),
);
const replySegments = computed(() => segments.value.filter((segment) => isReplySegment(segment)));

const foldLabel = computed(() => {
  const language = props.language ?? "zh-CN";
  if (props.message.status === "done" && !props.message.content.trim()) {
    return language === "zh-CN" ? "思考过程（未输出正文）" : "Reasoning (No output)";
  }
  return tr(language, "worked");
});

const foldMeta = computed(() => {
  const language = props.language ?? "zh-CN";
  return summarizeWorkFoldMeta(
    visibleActivities.value,
    countReasoningSegments(props.message.workTimeline, props.message.reasoning),
    language,
  );
});

const lastSegmentId = computed(() => segments.value[segments.value.length - 1]?.id);

function isFirstReasoningSegment(index: number) {
  for (let i = 0; i < index; i++) {
    if (segments.value[i]?.type === "reasoning") return false;
  }
  return true;
}

function processSegmentCollapsible(segment: Extract<TimelineSegment, { type: "process" }>) {
  // The subagent card already shows title + child rows; wrapping it in another
  // "Subagents (N)" process chrome duplicates the same block.
  if (
    segment.activities.length === 1 &&
    SUBAGENT_TOOLS.has(segment.activities[0]?.toolName ?? "")
  ) {
    return false;
  }
  return isProcessSegmentCollapsible(segment.activities, visibleActivities.value);
}

function processHeadline(segment: Extract<TimelineSegment, { type: "process" }>) {
  const language = props.language ?? "zh-CN";
  return summarizeProcessActivities(segment.activities, language);
}

function isProcessOpen(id: string) {
  return processOpen.get(id) ?? false;
}

function toggleProcess(id: string) {
  userToggledProcess.add(id);
  processOpen.set(id, !isProcessOpen(id));
}

function handleFoldToggle(event: Event) {
  const target = event.currentTarget as HTMLDetailsElement | null;
  if (!target) return;
  foldOpen.value = target.open;
  foldPinned.value = target.open;
}

/** Collapsed by default; only expand while showcase work is actively running. */
function shouldAutoExpandProcess(segment: Extract<TimelineSegment, { type: "process" }>) {
  if (waitingForAskUser.value) return true;
  return segment.activities.some(
    (activity) => activity.status === "running" && SHOWCASE_KINDS.has(activity.kind),
  );
}

watch(
  () => streaming.value,
  (live) => {
    if (live) {
      foldPinned.value = false;
      foldOpen.value = false;
      return;
    }
    // Always collapse the completed fold unless the user expands it afterward.
    if (!foldPinned.value) {
      foldOpen.value = false;
    }
  },
  { immediate: true },
);

watch(
  () =>
    [
      props.message.status,
      props.message.askUserAnswer?.length ?? 0,
      streaming.value,
      segments.value
        .map((segment) =>
          segment.type === "process"
            ? `${segment.id}:${segment.activities.map((a) => a.status).join(",")}`
            : segment.id,
        )
        .join("|"),
    ] as const,
  () => {
    for (const segment of segments.value) {
      if (segment.type !== "process") continue;
      if (userToggledProcess.has(segment.id)) continue;
      processOpen.set(segment.id, shouldAutoExpandProcess(segment));
    }
  },
  { immediate: true },
);

const conversationFind = inject(conversationFindKey, null);

function segmentMatchesQuery(segment: TimelineSegment, query: string) {
  if (
    segment.type === "reasoning" ||
    segment.type === "narration" ||
    segment.type === "content" ||
    segment.type === "inject"
  ) {
    return textIncludesQuery(segment.content, query);
  }
  if (segment.type === "ask-answer") {
    return segment.items.some(
      (item) =>
        textIncludesQuery(item.question ?? "", query) ||
        textIncludesQuery(item.header ?? "", query) ||
        item.selected.some((value) => textIncludesQuery(value, query)),
    );
  }
  return segment.activities.some((activity) => activityMatchesQuery(activity, query));
}

watch(
  () => [conversationFind?.active.value, conversationFind?.query.value, streaming.value] as const,
  ([active, query]) => {
    if (!active || !query?.trim()) return;
    if (preambleSegments.value.some((segment) => segmentMatchesQuery(segment, query))) {
      foldOpen.value = true;
      foldPinned.value = true;
    }
    for (const segment of segments.value) {
      if (segment.type !== "process") continue;
      if (!segment.activities.some((activity) => activityMatchesQuery(activity, query))) continue;
      userToggledProcess.add(segment.id);
      processOpen.set(segment.id, true);
    }
  },
  { immediate: true },
);
</script>

<style scoped>
.agent-work {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  margin-bottom: 6px;
  box-sizing: border-box;
}

.agent-work :deep(.shell-terminal-card),
.agent-work :deep(.file-diff-card),
.agent-work :deep(.task-list-card.embedded),
.agent-work :deep(.ask-answer-card) {
  margin-left: 0;
  margin-right: 0;
}

.agent-work :deep(.inject-card) {
  margin: 8px 0;
}

.agent-work-fold {
  width: 100%;
  margin: 0;
  border: 0;
  background: transparent;
}

.agent-work-fold-summary {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 28px;
  margin: 2px 0;
  cursor: pointer;
  padding: 4px 10px;
  border-radius: 6px;
  font-family: var(--peek-font-sans);
  font-size: var(--peek-font-sm, 12px);
  font-weight: 500;
  color: var(--peek-muted);
  list-style: none;
  user-select: none;
}

/* While expanded, keep the collapse control reachable no matter how long the
   folded reasoning/tool timeline is — pin it to the top of the scroll
   viewport instead of letting it scroll away with the rest of the body. */
.agent-work-fold-summary.pinned {
  position: sticky;
  top: var(--code-block-sticky-top, 0);
  z-index: 3;
  background: var(--peek-bg);
  margin: 0;
  padding: 6px 10px;
  border-radius: 6px;
}

.agent-work-fold-summary::-webkit-details-marker {
  display: none;
}

.agent-work-fold-chevron {
  flex: none;
  color: var(--peek-faint);
  transition: transform 160ms ease;
}

.agent-work-fold-chevron.open {
  transform: rotate(90deg);
}

.agent-work-fold-meta {
  margin-left: auto;
  padding-left: 8px;
  font-weight: 500;
  font-size: 11px;
  color: var(--peek-faint);
  white-space: nowrap;
}

.agent-work-fold-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px 10px 6px;
}

.agent-work.has-running-subagent .agent-work-fold-summary {
  color: var(--peek-text);
}
</style>
