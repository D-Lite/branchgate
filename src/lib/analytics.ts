import posthog from "posthog-js";
import { invoke } from "@tauri-apps/api/core";

const POSTHOG_KEY =
  (import.meta.env.VITE_POSTHOG_KEY as string | undefined) ??
  "phc_BskG7i7BSi7VRhgLoLrCfW5eDaKwGWLXoP9sQCwzLUrd";
const POSTHOG_HOST =
  (import.meta.env.VITE_POSTHOG_HOST as string | undefined) ??
  "https://us.i.posthog.com";

let initialized = false;

export function isAnalyticsAvailable(): boolean {
  return Boolean(POSTHOG_KEY.trim());
}

async function handleFirstLaunch() {
  if (await invoke<boolean>("get_first_launch_done")) return;

  const incomingId = await invoke<string | null>("get_pending_distinct_id");
  if (incomingId) {
    posthog.identify(incomingId);
  }

  posthog.capture("app:launched");
  await invoke("mark_first_launch_done");
}

export async function initAnalytics() {
  if (!isAnalyticsAvailable()) return;

  if (!initialized) {
    posthog.init(POSTHOG_KEY, {
      api_host: POSTHOG_HOST,
      autocapture: false,
      capture_pageview: false,
      capture_pageleave: false,
      disable_session_recording: true,
      persistence: "localStorage",
      opt_out_capturing_by_default: true,
      person_profiles: "identified_only",
    });
    posthog.register({ site_name: "desktop_app" });
    initialized = true;
  }

  const optedIn = await invoke<boolean>("get_analytics_opt_in");
  if (!optedIn) return;

  posthog.opt_in_capturing();
  await handleFirstLaunch();
}

export async function setAnalyticsOptIn(enabled: boolean) {
  await invoke("set_analytics_opt_in", { enabled });
  if (!initialized && isAnalyticsAvailable()) {
    await initAnalytics();
    return;
  }
  if (!initialized) return;

  if (enabled) {
    posthog.opt_in_capturing();
    await handleFirstLaunch();
  } else {
    posthog.opt_out_capturing();
  }
}

function capture(event: string, properties?: Record<string, string | number | boolean>) {
  if (!initialized) return;
  posthog.capture(event, properties);
}

export const track = {
  pipelineCreated: () => capture("app:pipeline_created"),
  promotionRunStarted: (prCount: number) =>
    capture("app:promotion_run_started", { pr_count: prCount }),
  promotionRunCompleted: (prCount: number, hadConflict: boolean) =>
    capture("app:promotion_run_completed", {
      pr_count: prCount,
      had_conflict: hadConflict,
    }),
  conflictEditorOpened: (editor: string) =>
    capture("app:conflict_editor_opened", { editor }),
};
