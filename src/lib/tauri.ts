import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  dbReady: boolean;
}

export interface Pipeline {
  id: number;
  name: string;
  sourceBranch: string;
  targetBranch: string;
  repoOwner: string | null;
  repoName: string | null;
  pendingCount: number;
}

export interface ConnectedRepo {
  id: number;
  kind: string;
  name: string | null;
  localPath: string | null;
  workingCopyMode: string;
  defaultBranch: string | null;
  defaultMergeStrategy: string;
  gitBackend: "auto" | "native" | "wsl";
  wslDistro: string | null;
  createdAt: number;
  pipelineCount: number;
}

export interface AppSettings {
  notifyOnConflict: boolean;
  notifyOnComplete: boolean;
  defaultMergeStrategy: string;
  managedClonesRoot: string | null;
  shareAnonymousUsage: boolean;
  analyticsConsentDecided: boolean;
}

export interface EditorInfo {
  id: number;
  name: string;
  command: string;
  detectedPath: string | null;
  isPreferred: boolean;
  lastVerifiedAt: number | null;
}

export interface DiagnosticsInfo {
  dbPath: string;
  repoCount: number;
  pipelineCount: number;
  editorCount: number;
  githubConnected: boolean;
  mode: string;
}

export type MergeStrategy = "auto" | "merge" | "squash" | "rebase";

export interface LocalRepoInfo {
  path: string;
  name: string;
  branches: string[];
  defaultBranch: string | null;
  existingRepoId: number | null;
}

export interface ConnectLocalRequest {
  localPath: string;
  pipelineName: string;
  sourceBranch: string;
  targetBranch: string;
  repoId?: number | null;
}

export interface SyncSummary {
  pipelineId: number;
  sourceHead: string;
  targetHead: string;
  unitsFound: number;
  pendingCount: number;
  promotedCount: number;
  syncedAt: number;
}

export interface PipelineSyncStatus {
  needsSync: boolean;
  syncedAt: number | null;
  sourceHead: string | null;
  targetHead: string | null;
  pendingCount: number;
  promotedCount: number;
}

export interface ChecklistItem {
  prId: number;
  mergeCommitSha: string;
  title: string;
  author: string | null;
  ticketRef: string | null;
  mergeStrategy: string | null;
  mergedAt: number | null;
  status: string;
  filesChanged: number;
  insertions: number;
  deletions: number;
  changedFiles: string[];
}

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export async function listPipelines(): Promise<Pipeline[]> {
  return invoke<Pipeline[]>("list_pipelines");
}

export async function deletePipeline(pipelineId: number): Promise<void> {
  return invoke("delete_pipeline", { pipelineId });
}

export async function listConnectedRepos(): Promise<ConnectedRepo[]> {
  return invoke<ConnectedRepo[]>("list_connected_repos");
}

export async function inspectLocalRepo(path: string): Promise<LocalRepoInfo> {
  return invoke<LocalRepoInfo>("inspect_local_repo", { path });
}

export async function connectLocalRepo(
  request: ConnectLocalRequest,
): Promise<Pipeline> {
  return invoke<Pipeline>("connect_local_repo", { request });
}

export async function syncPipeline(pipelineId: number): Promise<SyncSummary> {
  return invoke<SyncSummary>("sync_pipeline", { pipelineId });
}

export async function getPipelineChecklist(
  pipelineId: number,
): Promise<ChecklistItem[]> {
  return invoke<ChecklistItem[]>("get_pipeline_checklist", { pipelineId });
}

export async function getPipelineSyncStatus(
  pipelineId: number,
): Promise<PipelineSyncStatus> {
  return invoke<PipelineSyncStatus>("get_pipeline_sync_status", { pipelineId });
}

export interface PromotionRunItem {
  prId: number;
  title: string;
  mergeCommitSha: string;
  status: string;
  errorMessage: string | null;
  conflictFiles: string[];
}

export interface PromotionRunResult {
  runId: number;
  branchName: string;
  targetBranch: string;
  conflictPhase: string | null;
  status: string;
  items: PromotionRunItem[];
  preferredEditor: string | null;
  canContinue: boolean;
  recoverable: boolean;
}

export async function startPromotionRun(
  pipelineId: number,
  prIds: number[],
): Promise<PromotionRunResult> {
  return invoke<PromotionRunResult>("start_promotion_run", { pipelineId, prIds });
}

export async function abortPromotionRun(runId: number): Promise<void> {
  return invoke<void>("abort_promotion_run", { runId });
}

export async function getActivePromotionRun(
  pipelineId: number,
): Promise<PromotionRunResult | null> {
  return invoke<PromotionRunResult | null>("get_active_promotion_run", { pipelineId });
}

export async function continuePromotionRun(
  runId: number,
): Promise<PromotionRunResult> {
  return invoke<PromotionRunResult>("continue_promotion_run", { runId });
}

export async function refreshPromotionRun(
  runId: number,
): Promise<PromotionRunResult> {
  return invoke<PromotionRunResult>("refresh_promotion_run", { runId });
}

export async function openConflictInEditor(
  runId: number,
  filePath: string,
): Promise<string> {
  return invoke<string>("open_conflict_in_editor", { runId, filePath });
}

export async function openRepoInEditor(runId: number): Promise<void> {
  return invoke<void>("open_repo_in_editor", { runId });
}

export interface HistoryRunItem {
  prId: number;
  title: string;
  status: string;
}

export interface HistoryRun {
  runId: number;
  pipelineId: number;
  pipelineName: string;
  sourceBranch: string;
  targetBranch: string;
  branchName: string;
  status: string;
  conflictPhase: string | null;
  createdAt: number;
  completedAt: number | null;
  itemCount: number;
  items: HistoryRunItem[];
}

export async function listPromotionHistory(): Promise<HistoryRun[]> {
  return invoke<HistoryRun[]>("list_promotion_history");
}

export async function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_app_settings");
}

export async function updateAppSettings(
  request: Partial<AppSettings>,
): Promise<AppSettings> {
  return invoke<AppSettings>("update_app_settings", { request });
}

export async function listEditors(): Promise<EditorInfo[]> {
  return invoke<EditorInfo[]>("list_editors");
}

export async function detectEditors(): Promise<EditorInfo[]> {
  return invoke<EditorInfo[]>("detect_editors");
}

export async function setPreferredEditor(editorId: number): Promise<EditorInfo[]> {
  return invoke<EditorInfo[]>("set_preferred_editor", { editorId });
}

export async function updateRepoSettings(request: {
  repoId: number;
  workingCopyMode?: string;
  defaultMergeStrategy?: MergeStrategy;
  gitBackend?: "auto" | "native" | "wsl";
  wslDistro?: string | null;
}): Promise<void> {
  return invoke<void>("update_repo_settings", { request });
}

export async function disconnectRepo(repoId: number): Promise<void> {
  return invoke<void>("disconnect_repo", { repoId });
}

export async function clearSyncCache(): Promise<void> {
  return invoke<void>("clear_sync_cache");
}

export async function resetAppData(): Promise<void> {
  return invoke<void>("reset_app_data");
}

export async function getDiagnostics(): Promise<DiagnosticsInfo> {
  return invoke<DiagnosticsInfo>("get_diagnostics");
}

export const MERGE_STRATEGIES: { value: MergeStrategy; label: string; hint: string }[] = [
  { value: "auto", label: "Auto-detect", hint: "Infer from commit shape during sync" },
  { value: "merge", label: "Merge commits", hint: "Treat promotions as merge commits (-m 1)" },
  { value: "squash", label: "Squash merges", hint: "Treat promotions as single commits" },
  { value: "rebase", label: "Rebase merges", hint: "Linear cherry-picks without -m 1" },
];

export function platformClass(os: string): string {
  if (os === "macos") return "platform-macos";
  if (os === "windows") return "platform-windows";
  return "platform-other";
}

export function defaultPipelineName(
  repoName: string,
  source: string,
  target: string,
): string {
  return `${repoName}: ${source} → ${target}`;
}

export function suggestBranches(branches: string[]): {
  source: string;
  target: string;
} {
  const normalized = branches.map((b) => b.toLowerCase());
  const pick = (...candidates: string[]) => {
    for (const candidate of candidates) {
      const idx = normalized.indexOf(candidate);
      if (idx >= 0) return branches[idx];
    }
    return null;
  };

  const source =
    pick("develop", "dev", "main", "master") ?? branches[0] ?? "";
  const target =
    pick("staging", "stage", "main", "master", "production", "prod") ??
    branches.find((b) => b !== source) ??
    branches[1] ??
    "";

  return { source, target };
}

export function formatRelativeTime(epochSecs: number | null): string {
  if (!epochSecs) return "—";
  const date = new Date(epochSecs * 1000);
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}
