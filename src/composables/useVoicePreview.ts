import { onUnmounted, ref } from "vue";
import { api } from "@/api";
import type { EngineProfile, ParamValue } from "@/api/types";

export interface PreviewRequest {
  profile: EngineProfile;
  voiceId: string | null;
  params: Record<string, ParamValue>;
  language: string | null;
}

/** Plays short WAV samples the core makes. Each is cached by its key while the component lives,
 *  and playback stops when it unmounts. */
export function useSample() {
  const state = ref<"idle" | "loading" | "playing">("idle"),
    error = ref("");
  const cache = new Map<string, string>();
  let audio: HTMLAudioElement | null = null,
    request = 0;
  function stop() {
    request += 1;
    audio?.pause();
    audio = null;
    state.value = "idle";
  }
  async function play(key: string, make: () => Promise<ArrayBuffer>) {
    stop();
    const id = request;
    error.value = "";
    state.value = "loading";
    try {
      let url = cache.get(key);
      if (!url) {
        const wav = await make();
        url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
        cache.set(key, url);
      }
      if (id !== request) return;
      const sample = new Audio(url);
      sample.onended = () => {
        if (id === request) state.value = "idle";
      };
      audio = sample;
      await sample.play();
      if (id === request) state.value = "playing";
    } catch (e) {
      if (id !== request) return;
      error.value = String(e);
      state.value = "idle";
    }
  }
  onUnmounted(() => {
    stop();
    cache.forEach((url) => URL.revokeObjectURL(url));
  });
  return { state, error, play, stop };
}

/** Plays a voice sample for engine settings that may not be saved yet. */
export function useVoicePreview() {
  const sample = useSample();
  function play({ profile, voiceId, params, language }: PreviewRequest) {
    const plain = JSON.parse(JSON.stringify(profile)) as EngineProfile;
    return sample.play(JSON.stringify([plain, voiceId, params, language]), () =>
      api.previewVoice({ profile: plain, voiceId, params: { ...params }, language }),
    );
  }
  return { ...sample, play };
}
