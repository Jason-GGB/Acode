<template>
  <span
    class="mascot-face"
    :class="[
      `st-${visualState}`,
      tool ? `tool-${tool}` : '',
      { interactive, pleased, turning, 'eyes-only': eyesOnly, watching },
    ]"
    :role="alt ? 'img' : undefined"
    :aria-label="alt || undefined"
    :aria-hidden="alt ? undefined : true"
    @click="handleClick"
  >
    <img class="mascot-portrait" src="/mascot/mascot.png" alt="" draggable="false" />
  </span>
</template>

<script lang="ts">
/** Items the mascot can hold; the caller maps app state (e.g. chat mode) onto these. */
export type MascotTool = "phone" | "wrench" | "clipboard" | "brush";

/**
 * Facial state, roughly mirroring an agent turn:
 * idle / thinking / working / talking / waiting / done / error.
 */
export type MascotState =
  "idle" | "thinking" | "working" | "talking" | "waiting" | "done" | "error";
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";

/** Portrait mascot. Callers still pass state, tool, and gaze hints for status styling. */
const props = withDefaults(
  defineProps<{
    alt?: string;
    /** Element or viewport point the face should attend to. */
    lookAt?: HTMLElement | { x: number; y: number } | null;
    /** Follow the pointer while it is inside the window. */
    followPointer?: boolean;
    /** Clicking the face plays a short pleased motion. */
    interactive?: boolean;
    /** Facial state; see MascotState. */
    state?: MascotState;
    /** Play a short turn when `state` changes. */
    turnOnChange?: boolean;
    /** Shorthand for `state="working"`. */
    busy?: boolean;
    /** Item carried with the mascot; null hides the tool marker. */
    tool?: MascotTool | null;
    /** Crop tighter on the face. */
    eyesOnly?: boolean;
  }>(),
  {
    alt: "",
    lookAt: null,
    followPointer: true,
    interactive: false,
    state: "idle",
    turnOnChange: true,
    busy: false,
    tool: null,
    eyesOnly: false,
  },
);

const emit = defineEmits<{ click: [] }>();

const pleased = ref(false);
const turning = ref(false);
let pleasedTimer: ReturnType<typeof setTimeout> | null = null;
let turnTimer: ReturnType<typeof setTimeout> | null = null;

const watching = computed(() => props.followPointer || props.lookAt != null);
const visualState = computed<MascotState>(() => {
  if (pleased.value) return "done";
  if (props.busy && props.state === "idle") return "working";
  return props.state;
});

watch(
  () => props.state,
  () => {
    if (!props.turnOnChange) return;
    turning.value = true;
    if (turnTimer) clearTimeout(turnTimer);
    turnTimer = setTimeout(() => {
      turning.value = false;
    }, 280);
  },
);

function handleClick() {
  if (!props.interactive) return;
  pleased.value = true;
  emit("click");
  if (pleasedTimer) clearTimeout(pleasedTimer);
  pleasedTimer = setTimeout(() => {
    pleased.value = false;
  }, 700);
}

onBeforeUnmount(() => {
  if (pleasedTimer) clearTimeout(pleasedTimer);
  if (turnTimer) clearTimeout(turnTimer);
});
</script>

<style scoped>
.mascot-face {
  display: block;
  width: 100%;
  height: 100%;
}
.mascot-face.interactive {
  cursor: pointer;
  user-select: none;
  -webkit-tap-highlight-color: transparent;
}
.mascot-portrait {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  object-position: center 38%;
  border-radius: 22%;
  pointer-events: none;
  transform-origin: 50% 42%;
  transition: transform 280ms cubic-bezier(0.34, 1.4, 0.64, 1);
}
.eyes-only .mascot-portrait {
  object-position: center 28%;
  border-radius: 28%;
}
.pleased .mascot-portrait {
  transform: rotate(-4deg) scale(1.04);
}
.turning .mascot-portrait {
  transform: scaleX(0.92);
}
.st-working .mascot-portrait {
  animation: mascot-bob 1.6s ease-in-out infinite;
}
.st-error .mascot-portrait {
  filter: saturate(0.72);
}
@keyframes mascot-bob {
  0%,
  100% {
    transform: translateY(0);
  }
  50% {
    transform: translateY(-4%);
  }
}
@media (prefers-reduced-motion: reduce) {
  .mascot-portrait,
  .st-working .mascot-portrait,
  .turning .mascot-portrait,
  .pleased .mascot-portrait {
    animation: none;
    transition: none;
    transform: none;
  }
}
</style>
