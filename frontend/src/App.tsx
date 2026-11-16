import { useQueryClient } from "@tanstack/react-query";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  addAccount,
  createReminder,
  createTaskReminder,
  deleteAccount,
  deleteReminder,
  lookupPigaiEssay,
  openTask,
  refreshAccount,
  refreshAll,
  restoreTask,
  updateReminder,
  updateTaskStatus,
} from "./api";
import { AppShell, type ViewId } from "./components/AppShell";
import { TaskReminderDialog } from "./components/TaskReminderDialog";
import {
  Button,
  ErrorState,
  LoadingState,
  Toast,
} from "./components/Primitives";
import { accountName } from "./lib/format";
import {
  isReminderPayload,
  REMINDER_EVENT,
  reminderToastMessage,
} from "./reminder-events";
import { useWorkspace, workspaceQueryKey } from "./query";
import type {
  Account,
  AccountInput,
  CompletedTask,
  Reminder,
  RepeatRule,
  Task,
  TaskStatus,
  WorkspaceData,
} from "./types";
import { AccountsView } from "./views/AccountsView";
import { CompletedView } from "./views/CompletedView";
import { OverviewView } from "./views/OverviewView";
import { RemindersView } from "./views/RemindersView";
import { TasksView } from "./views/TasksView";

type ToastState = {
  message: string;
  kind: "success" | "error" | "info";
};

export function App() {
  const queryClient = useQueryClient();
  const workspace = useWorkspace();
  const [view, setView] = useState<ViewId>("overview");
  const [toast, setToast] = useState<ToastState | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [refreshingAccountId, setRefreshingAccountId] = useState<string | null>(null);
  const [accountFormOpen, setAccountFormOpen] = useState(false);
  const [reminderTask, setReminderTask] = useState<Task | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current);
    },
    [],
  );

  const notify = useCallback((message: string, kind: ToastState["kind"] = "success") => {
    if (toastTimer.current !== undefined) window.clearTimeout(toastTimer.current);
    setToast({ message, kind });
    toastTimer.current = window.setTimeout(() => setToast(null), 4200);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<unknown>(REMINDER_EVENT, (event) => {
          if (disposed || !isReminderPayload(event.payload)) return;
          notify(reminderToastMessage(event.payload), "info");
          void queryClient.invalidateQueries({ queryKey: workspaceQueryKey });
        }),
      )
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => {
        // Browser builds use polling; only the desktop event bridge is optional.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [notify, queryClient]);

  const refreshWorkspace = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: workspaceQueryKey });
    await queryClient.refetchQueries({ queryKey: workspaceQueryKey });
  }, [queryClient]);

  const syncAll = useCallback(async () => {
    if (syncing) return;
    setSyncing(true);
    notify("正在同步全部学习账号", "info");
    try {
      const results = await refreshAll();
      await refreshWorkspace();
      const succeeded = results.filter((result) => result.ok).length;
      const failed = results.length - succeeded;
      notify(
        failed
          ? `同步完成：${succeeded} 个成功，${failed} 个需要处理`
          : `已同步 ${succeeded} 个账号`,
        failed ? "error" : "success",
      );
    } catch (cause) {
      notify(readError(cause, "全部账号同步失败"), "error");
    } finally {
      setSyncing(false);
    }
  }, [notify, refreshWorkspace, syncing]);

  const syncAccount = useCallback(
    async (account: Account) => {
      if (refreshingAccountId || syncing) return;
      setRefreshingAccountId(account.id);
      notify(`正在同步 ${accountName(account)}`, "info");
      try {
        await refreshAccount(account.id);
        await refreshWorkspace();
        notify(`${accountName(account)} 已同步`);
      } catch (cause) {
        await refreshWorkspace();
        notify(readError(cause, "账号同步失败"), "error");
      } finally {
        setRefreshingAccountId(null);
      }
    },
    [notify, refreshWorkspace, refreshingAccountId, syncing],
  );

  const changeTaskStatus = useCallback(
    async (task: Task, status: TaskStatus) => {
      try {
        await updateTaskStatus(task.id, status);
        await refreshWorkspace();
        notify(
          status === "completed"
            ? "任务已完成"
            : status === "ignored"
              ? "任务已忽略"
              : "任务已恢复为待完成",
        );
      } catch (cause) {
        notify(readError(cause, "任务状态更新失败"), "error");
      }
    },
    [notify, refreshWorkspace],
  );

  const restoreCompleted = useCallback(
    async (task: CompletedTask) => {
      try {
        await restoreTask(task.id);
        await refreshWorkspace();
        notify("任务已恢复为待完成");
      } catch (cause) {
        notify(readError(cause, "恢复任务失败"), "error");
      }
    },
    [notify, refreshWorkspace],
  );

  const open = useCallback(
    async (task: Task) => {
      try {
        await openTask(task);
      } catch (cause) {
        notify(readError(cause, "无法打开学习平台"), "error");
      }
    },
    [notify],
  );

  const createAccountAndSync = useCallback(
    async (input: AccountInput) => {
      const account = await addAccount(input);
      try {
        await refreshAccount(account.id);
        notify("账号已添加并完成同步");
      } catch (cause) {
        notify(`账号已添加，但同步失败：${readError(cause, "请稍后重试")}`, "error");
      }
      await refreshWorkspace();
    },
    [notify, refreshWorkspace],
  );

  const removeAccount = useCallback(
    async (account: Account) => {
      const confirmed = window.confirm(
        `确定删除账号“${accountName(account)}”吗？该账号的本地课程、任务和提醒也会一并移除。`,
      );
      if (!confirmed) return;
      try {
        await deleteAccount(account.id);
        await refreshWorkspace();
        notify("账号及其本地数据已移除");
      } catch (cause) {
        notify(readError(cause, "删除账号失败"), "error");
      }
    },
    [notify, refreshWorkspace],
  );

  const addTaskReminder = useCallback(
    async (
      task: Task,
      input: {
        mode: "countdown" | "absolute";
        offset_minutes?: number;
        remind_at?: string;
      },
    ) => {
      await createTaskReminder(task.id, input);
      await refreshWorkspace();
      notify("任务提醒已创建");
    },
    [notify, refreshWorkspace],
  );

  const addStandaloneReminder = useCallback(
    async (input: { title: string; remind_at: string; repeat_rule: RepeatRule }) => {
      await createReminder(input);
      await refreshWorkspace();
      notify("提醒已添加到计划");
    },
    [notify, refreshWorkspace],
  );

  const toggleReminder = useCallback(
    async (reminder: Reminder) => {
      try {
        await updateReminder(reminder.id, !reminder.enabled);
        await refreshWorkspace();
        notify(reminder.enabled ? "提醒已停用" : "提醒已重新启用");
      } catch (cause) {
        notify(readError(cause, "提醒状态更新失败"), "error");
      }
    },
    [notify, refreshWorkspace],
  );

  const removeReminder = useCallback(
    async (reminder: Reminder) => {
      try {
        await deleteReminder(reminder.id);
        await refreshWorkspace();
        notify("提醒已删除");
      } catch (cause) {
        notify(readError(cause, "删除提醒失败"), "error");
      }
    },
    [notify, refreshWorkspace],
  );

  const lookupEssay = useCallback(
    async (accountId: string, essayId: string) => {
      await lookupPigaiEssay(accountId, essayId);
      await refreshWorkspace();
      notify("作文已添加到学习任务");
    },
    [notify, refreshWorkspace],
  );

  if (workspace.isPending) {
    return (
      <div className="boot-screen">
        <div className="boot-brand">
          <span className="brand-mark">S</span>
          <strong>StudyPilot</strong>
        </div>
        <LoadingState />
      </div>
    );
  }
  if (workspace.isError || !workspace.data) {
    return (
      <div className="boot-screen">
        <div className="boot-brand">
          <span className="brand-mark">S</span>
          <strong>StudyPilot</strong>
        </div>
        <ErrorState
          message={readError(workspace.error, "无法加载本地学习数据")}
          onRetry={() => void workspace.refetch()}
        />
      </div>
    );
  }

  const data: WorkspaceData = workspace.data;
  let content;
  if (view === "overview") {
    content = (
      <OverviewView
        data={data}
        onView={setView}
        onOpenTask={open}
        onSyncAll={syncAll}
        syncing={syncing}
      />
    );
  } else if (view === "tasks") {
    content = (
      <TasksView
        data={data}
        onOpenTask={open}
        onStatus={changeTaskStatus}
        onRemind={setReminderTask}
        onLookup={lookupEssay}
      />
    );
  } else if (view === "completed") {
    content = <CompletedView data={data} onOpen={open} onRestore={restoreCompleted} />;
  } else if (view === "accounts") {
    content = (
      <AccountsView
        data={data}
        formOpen={accountFormOpen}
        onFormOpenChange={setAccountFormOpen}
        onAdd={createAccountAndSync}
        onRefresh={syncAccount}
        onRefreshAll={syncAll}
        onDelete={removeAccount}
        syncing={syncing}
        pendingAccountId={refreshingAccountId}
      />
    );
  } else {
    content = (
      <RemindersView
        data={data}
        onCreate={addStandaloneReminder}
        onToggle={toggleReminder}
        onDelete={removeReminder}
      />
    );
  }

  return (
    <>
      <AppShell
        view={view}
        onView={setView}
        data={data}
        syncing={syncing}
        onSyncAll={syncAll}
        onAddAccount={() => {
          setAccountFormOpen(true);
          setView("accounts");
        }}
        onRefresh={() => setView("accounts")}
      >
        {workspace.isFetching && !workspace.isPending && (
          <div className="refreshing-line" role="status">
            <LoaderCircle size={14} className="spin" />
            正在更新学习数据
          </div>
        )}
        {content}
      </AppShell>
      {reminderTask && (
        <TaskReminderDialog
          task={reminderTask}
          onClose={() => setReminderTask(null)}
          onSubmit={(input) => addTaskReminder(reminderTask, input)}
        />
      )}
      {toast && (
        <Toast
          message={toast.message}
          kind={toast.kind}
          onClose={() => setToast(null)}
        />
      )}
    </>
  );
}

function readError(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}
