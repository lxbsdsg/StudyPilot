import { isTauriRuntime } from "./capabilities";
import type {
  Account,
  AccountInput,
  RefreshResult,
  Reminder,
  RepeatRule,
  Task,
  TaskStatus,
  WorkspaceData,
} from "./types";

const jsonHeaders = { "Content-Type": "application/json" };
type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

let tauriInvoke: TauriInvoke | undefined;

export async function requestHttp<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: { ...jsonHeaders, ...(init?.headers ?? {}) },
  });
  if (!response.ok) {
    let message = `请求失败 (${response.status})`;
    try {
      const body = (await response.json()) as { detail?: string; error?: string };
      message = body.detail || body.error || message;
    } catch {
      // Keep the HTTP status when the server returns a non-JSON error.
    }
    throw new Error(message);
  }
  if (response.status === 204) return undefined as T;
  if (response.status === 200) {
    const body = (await response.json()) as T | null;
    if (body === null) return undefined as T;
    return body;
  }
  return response.json() as Promise<T>;
}

export async function requestDesktop<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!tauriInvoke) {
    const api = await import("@tauri-apps/api/core");
    tauriInvoke = api.invoke;
  }
  return tauriInvoke<T>(command, args);
}

export async function fetchWorkspace(): Promise<WorkspaceData> {
  if (isTauriRuntime()) return requestDesktop("fetch_workspace");
  return requestHttp<WorkspaceData>("/api/workspace");
}

export async function addAccount(input: AccountInput): Promise<Account> {
  if (isTauriRuntime()) return requestDesktop("add_account", { payload: input });
  return requestHttp<Account>("/api/accounts", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function refreshAccount(accountId: string): Promise<RefreshResult> {
  if (isTauriRuntime()) return requestDesktop("refresh_account", { accountId });
  return requestHttp<RefreshResult>(`/api/accounts/${accountId}/refresh`, {
    method: "POST",
  });
}

export async function refreshAll(): Promise<RefreshResult[]> {
  if (isTauriRuntime()) return requestDesktop("refresh_all");
  return requestHttp<RefreshResult[]>("/api/accounts/refresh", {
    method: "POST",
    body: "{}",
  });
}

export async function deleteAccount(accountId: string): Promise<void> {
  if (isTauriRuntime()) return requestDesktop("delete_account", { accountId });
  return requestHttp<void>(`/api/accounts/${accountId}`, { method: "DELETE" });
}

export async function updateTaskStatus(taskId: string, status: TaskStatus) {
  if (isTauriRuntime()) {
    return requestDesktop<{ ok: boolean }>("update_task_status", { taskId, status });
  }
  return requestHttp<{ ok: boolean }>(`/api/tasks/${taskId}/status`, {
    method: "POST",
    body: JSON.stringify({ status }),
  });
}

export async function restoreTask(taskId: string) {
  if (isTauriRuntime()) return requestDesktop<{ ok: boolean }>("restore_task", { taskId });
  return requestHttp<{ ok: boolean }>(`/api/completed-tasks/${taskId}/restore`, {
    method: "POST",
  });
}

export async function openTask(task: Task) {
  const isMobile = /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent);
  const target = isTauriRuntime()
    ? await requestDesktop<{ url?: string; web_url?: string }>("task_open_target", {
        accountId: task.account_id,
        taskId: task.id,
        mobile: isMobile,
      })
    : await requestHttp<{ url?: string; web_url?: string }>("/api/open", {
        method: "POST",
        body: JSON.stringify({
          account_id: task.account_id,
          task_id: task.id,
          mobile: isMobile,
        }),
      });
  const url = target.web_url || target.url;
  if (!url) return;
  if (isTauriRuntime()) {
    const opener = await import("@tauri-apps/plugin-opener");
    await opener.openUrl(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

export async function lookupPigaiEssay(accountId: string, essayId: string) {
  if (isTauriRuntime()) {
    return requestDesktop<Task>("lookup_pigai_essay", { accountId, essayId });
  }
  return requestHttp<Task>("/api/pigai/essay", {
    method: "POST",
    body: JSON.stringify({ account_id: accountId, essay_id: essayId }),
  });
}

interface TaskReminderInput {
  mode: "countdown" | "absolute";
  offset_minutes?: number;
  remind_at?: string;
}

export async function createTaskReminder(taskId: string, input: TaskReminderInput) {
  if (isTauriRuntime()) {
    return requestDesktop<Reminder>("create_task_reminder", { taskId, payload: input });
  }
  return requestHttp<Reminder>(`/api/tasks/${taskId}/reminder`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function createReminder(input: {
  title: string;
  remind_at: string;
  repeat_rule: RepeatRule;
}) {
  if (isTauriRuntime()) return requestDesktop<Reminder>("create_reminder", { payload: input });
  return requestHttp<Reminder>("/api/reminders", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function updateReminder(reminderId: string, enabled: boolean) {
  if (isTauriRuntime()) {
    return requestDesktop<Reminder>("update_reminder", {
      reminderId,
      payload: { enabled },
    });
  }
  return requestHttp<Reminder>(`/api/reminders/${reminderId}`, {
    method: "PATCH",
    body: JSON.stringify({ enabled }),
  });
}

export async function deleteReminder(reminderId: string): Promise<void> {
  if (isTauriRuntime()) {
    await requestDesktop("delete_reminder", { reminderId });
    return;
  }
  return requestHttp<void>(`/api/reminders/${reminderId}`, { method: "DELETE" });
}
