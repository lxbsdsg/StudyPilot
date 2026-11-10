const state = {
  platforms: [],
  accounts: [],
  courses: [],
  tasks: [],
  reminders: [],
  completed: [],
  dashboard: null,
  view: "overview",
  reminderTaskId: null,
};

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];

async function api(path, options = {}) {
  const response = await fetch(path, {
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
    ...options,
  });
  if (!response.ok) {
    let message = `请求失败 (${response.status})`;
    try {
      const body = await response.json();
      message = body.detail || body.error || message;
    } catch (_) {}
    throw new Error(message);
  }
  if (response.status === 204) return null;
  return response.json();
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function platformLabel(platform) {
  return state.platforms.find((item) => item.id === platform)?.label || platform;
}

function accountFor(accountId) {
  return state.accounts.find((item) => item.id === accountId);
}

function formatTime(value) {
  if (!value) return "—";
  const date = new Date(value.replace(" ", "T"));
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function deadlineClass(value) {
  if (!value) return "";
  const date = new Date(value.replace(" ", "T"));
  if (Number.isNaN(date.getTime())) return "";
  const diff = date.getTime() - Date.now();
  if (diff < 0) return "overdue";
  if (diff < 72 * 3600 * 1000) return "soon";
  return "";
}

function toast(message, error = false) {
  const element = $("#toast");
  element.textContent = message;
  element.classList.toggle("error", error);
  element.classList.add("show");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => element.classList.remove("show"), 2800);
}

function setView(view) {
  state.view = view;
  $$(".nav-item").forEach((item) => {
    item.classList.toggle("active", item.dataset.view === view);
  });
  $$(".view").forEach((item) => {
    item.classList.toggle("active", item.id === `view-${view}`);
  });
  const titles = {
    overview: ["学习总览", "集中查看各平台课程与待完成事项"],
    tasks: ["学习任务", "按平台、账号和截止时间筛选任务"],
    completed: ["已完成", "查看最近 7 天完成记录并恢复未完成"],
    accounts: ["账号管理", "同时登录多个学习平台账号"],
    reminders: ["提醒计划", "为截止任务设置定时通知"],
  };
  $("#viewTitle").textContent = titles[view][0];
  $("#viewSubtitle").textContent = titles[view][1];
}

function renderPlatformOptions() {
  const accountPlatform = $("#accountPlatform");
  const filterPlatform = $("#filterPlatform");
  accountPlatform.innerHTML = state.platforms
    .map((item) => `<option value="${item.id}">${item.label}</option>`)
    .join("");
  filterPlatform.innerHTML =
    '<option value="">全部平台</option>' +
    state.platforms
      .map((item) => `<option value="${item.id}">${item.label}</option>`)
      .join("");
}

function renderAccountFilter() {
  const selected = $("#filterAccount").value;
  $("#filterAccount").innerHTML =
    '<option value="">全部账号</option>' +
    state.accounts
      .map(
        (account) =>
          `<option value="${account.id}">${escapeHtml(
            account.label || account.display_name || account.username
          )} · ${platformLabel(account.platform)}</option>`
      )
      .join("");
  if ([...$("#filterAccount").options].some((item) => item.value === selected)) {
    $("#filterAccount").value = selected;
  }
}

function renderPigaiAccountOptions() {
  const selected = $("#pigaiAccount").value;
  const accounts = state.accounts.filter(
    (account) => account.platform === "pigai"
  );
  $("#pigaiAccount").innerHTML = accounts.length
    ? accounts
        .map(
          (account) =>
            `<option value="${account.id}">${escapeHtml(
              account.label || account.display_name || account.username
            )}</option>`
        )
        .join("")
    : '<option value="">请先添加批改网账号</option>';
  if ([...$("#pigaiAccount").options].some((item) => item.value === selected)) {
    $("#pigaiAccount").value = selected;
  }
  $("#pigaiLookupBtn").disabled = accounts.length === 0;
}

function renderDashboard() {
  const data = state.dashboard || {};
  $("#metricAccounts").textContent = data.accounts || 0;
  $("#metricCourses").textContent = data.courses || 0;
  $("#metricPending").textContent = data.pending || 0;
  $("#metricDue").textContent = data.due_soon || 0;
  $("#navTaskCount").textContent = data.pending || 0;
  $("#navCompletedCount").textContent = state.completed.length;

  const platforms = data.platforms || [];
  const checks = data.platform_checks || {};
  $("#platformList").className = platforms.length ? "platform-list" : "platform-list empty-state";
  $("#platformList").innerHTML = platforms.length
    ? platforms
        .map(
          (item) => `
            <div class="platform-row">
              <div>
                <div class="platform-name">${escapeHtml(item.label)}</div>
                <div class="platform-sub">${item.accounts} 个账号 · ${item.pending} 项待完成</div>
              </div>
              <span class="tag neutral">${item.pending} 待办</span>
              <span class="tag ${
                item.errors
                  ? "danger"
                  : checks[item.platform]?.status === "verified"
                  ? "success"
                  : "warning"
              }">${
                item.errors
                  ? `${item.errors} 异常`
                  : checks[item.platform]?.status === "verified"
                  ? "已验证"
                  : "待账号验证"
              }</span>
            </div>`
        )
        .join("")
    : "暂无账号";

  const nextTasks = data.next_tasks || [];
  $("#nextTasks").className = nextTasks.length ? "task-mini-list" : "task-mini-list empty-state";
  $("#nextTasks").innerHTML = nextTasks.length
    ? nextTasks
        .map(
          (task) => `
            <div class="task-mini">
              <div>
                <strong>${escapeHtml(task.title)}</strong>
                <small>${platformLabel(task.platform)} · ${escapeHtml(task.course_name || "未分类")}</small>
              </div>
              <span class="muted small">${formatTime(task.deadline)}</span>
            </div>`
        )
        .join("")
    : "暂无任务";
}

function renderCompleted() {
  const query = $("#completedSearch").value.trim().toLowerCase();
  const tasks = state.completed.filter((task) => {
    if (!query) return true;
    return `${task.title} ${task.course_name} ${task.detail}`
      .toLowerCase()
      .includes(query);
  });
  $("#completedEmpty").classList.toggle("hidden", tasks.length > 0);
  $("#completedTable").innerHTML = tasks
    .map((task) => {
      const account = accountFor(task.account_id);
      const completedAt = task.completed_at
        ? new Date(task.completed_at * 1000).toLocaleString("zh-CN", {
            month: "2-digit",
            day: "2-digit",
            hour: "2-digit",
            minute: "2-digit",
          })
        : "—";
      return `
        <tr>
          <td><span class="tag neutral">${escapeHtml(platformLabel(task.platform))}</span></td>
          <td class="title-cell">
            <strong>${escapeHtml(task.course_name || "未分类")}</strong>
            <small>${escapeHtml(account?.label || account?.username || "")}</small>
          </td>
          <td class="title-cell">
            <strong>${escapeHtml(task.title)}</strong>
            <small>${escapeHtml(task.detail || task.source)}</small>
          </td>
          <td>${completedAt}</td>
          <td class="deadline-cell">${formatTime(task.deadline)}</td>
          <td>
            <div class="actions">
              <button class="button small toolbar-button" data-open="${task.id}">查看</button>
              <button class="button small" data-restore="${task.id}">重新标记未完成</button>
            </div>
          </td>
        </tr>`;
    })
    .join("");
}

function filteredTasks() {
  const platform = $("#filterPlatform").value;
  const accountId = $("#filterAccount").value;
  const status = $("#filterStatus").value;
  const query = $("#taskSearch").value.trim().toLowerCase();
  return state.tasks.filter((task) => {
    if (platform && task.platform !== platform) return false;
    if (accountId && task.account_id !== accountId) return false;
    if (status && task.status !== status) return false;
    if (
      query &&
      !`${task.title} ${task.course_name} ${task.detail}`.toLowerCase().includes(query)
    ) {
      return false;
    }
    return true;
  });
}

function renderTasks() {
  const tasks = filteredTasks();
  $("#taskCountText").textContent = `${tasks.length} 项任务`;
  $("#taskEmpty").classList.toggle("hidden", tasks.length > 0);
  $("#taskTable").innerHTML = tasks
    .map((task) => {
      const account = accountFor(task.account_id);
      const statusTag =
        task.status === "completed"
          ? '<span class="tag success">已完成</span>'
          : task.status === "ignored"
          ? '<span class="tag neutral">已忽略</span>'
          : '<span class="tag warning">未完成</span>';
      const statusAction =
        task.status === "pending" ? "completed" : "pending";
      const statusLabel = task.status === "pending" ? "标为完成" : "恢复未完成";
      return `
        <tr>
          <td><span class="tag neutral">${escapeHtml(platformLabel(task.platform))}</span></td>
          <td class="title-cell">
            <strong>${escapeHtml(task.course_name || "未分类")}</strong>
            <small>${escapeHtml(account?.label || account?.username || "")}</small>
          </td>
          <td class="title-cell">
            <strong>${escapeHtml(task.title)}</strong>
            <small>${escapeHtml(task.detail || task.source)}</small>
          </td>
          <td class="deadline-cell">
            <span class="${deadlineClass(task.deadline)}">${formatTime(task.deadline)}</span>
          </td>
          <td>${statusTag}</td>
          <td>
            <div class="actions">
              <button class="button small toolbar-button" data-open="${task.id}">打开</button>
              <button class="button small" data-remind="${task.id}">提醒</button>
              <button class="button small" data-status="${task.id}" data-next="${statusAction}">${statusLabel}</button>
            </div>
          </td>
        </tr>`;
    })
    .join("");
}

function renderAccounts() {
  $("#accountList").className = state.accounts.length
    ? "account-list"
    : "account-list empty-state";
  $("#accountList").innerHTML = state.accounts.length
    ? state.accounts
        .map((account) => {
          const status =
            account.status === "ready"
              ? '<span class="tag success">正常</span>'
              : account.status === "refreshing"
              ? '<span class="tag">同步中</span>'
              : '<span class="tag danger">异常</span>';
          return `
            <div class="account-item">
              <div class="item-main">
                <strong>${escapeHtml(account.label || account.display_name || account.username)}</strong>
                <small>${escapeHtml(platformLabel(account.platform))} · ${escapeHtml(
            account.username
          )} ${account.school ? `· ${escapeHtml(account.school)}` : ""}</small>
                ${
                  account.last_error
                    ? `<p class="error-text">${escapeHtml(account.last_error)}</p>`
                    : ""
                }
              </div>
              <div class="item-actions">
                ${status}
                <button class="button small" data-refresh="${account.id}">刷新</button>
                <button class="button small danger" data-delete="${account.id}">移除</button>
              </div>
            </div>`;
        })
        .join("")
    : "尚未添加账号";
}

function renderReminders() {
  $("#reminderCountText").textContent = `${state.reminders.length} 个提醒`;
  $("#reminderList").className = state.reminders.length
    ? "reminder-list"
    : "reminder-list empty-state";
  $("#reminderList").innerHTML = state.reminders.length
    ? state.reminders
        .map(
          (reminder) => `
            <div class="reminder-item">
              <div class="item-main">
                <strong>${escapeHtml(reminder.title)}</strong>
                <small>${formatTime(reminder.remind_at)} · ${
            reminder.repeat_rule === "once"
              ? "仅一次"
              : reminder.repeat_rule === "daily"
              ? "每天"
              : reminder.repeat_rule === "weekdays"
              ? "每个工作日"
              : "每周"
          }${reminder.task_title ? ` · ${escapeHtml(reminder.task_title)}` : ""}</small>
              </div>
              <div class="item-actions">
                <span class="tag ${reminder.enabled ? "success" : "neutral"}">${
            reminder.enabled ? "已启用" : "已停用"
          }</span>
                <button class="button small" data-toggle-reminder="${reminder.id}" data-enabled="${
            reminder.enabled ? "0" : "1"
          }">${reminder.enabled ? "停用" : "启用"}</button>
                <button class="button small danger" data-delete-reminder="${reminder.id}">删除</button>
              </div>
            </div>`
        )
        .join("")
    : "暂无提醒";
}

function renderReminderTaskOptions() {
  const select = $("#reminderTask");
  select.innerHTML =
    '<option value="">请选择有截止时间的任务</option>' +
    state.tasks
      .filter((task) => task.status === "pending" && task.deadline)
      .map(
        (task) =>
          `<option value="${task.id}">${escapeHtml(platformLabel(task.platform))} · ${escapeHtml(
            task.title
          )} · ${formatTime(task.deadline)}</option>`
      )
      .join("");
}

async function loadAll() {
  const [platforms, accounts, courses, tasks, completed, reminders, dashboard] =
    await Promise.all([
    api("/api/platforms"),
    api("/api/accounts"),
    api("/api/courses"),
    api("/api/tasks"),
    api("/api/completed-tasks?days=7"),
    api("/api/reminders"),
    api("/api/dashboard"),
    ]);
  state.platforms = platforms;
  state.accounts = accounts;
  state.courses = courses;
  state.tasks = tasks;
  state.completed = completed;
  state.reminders = reminders;
  state.dashboard = dashboard;
  renderPlatformOptions();
  renderAccountFilter();
  renderPigaiAccountOptions();
  renderDashboard();
  renderTasks();
  renderCompleted();
  renderAccounts();
  renderReminders();
  renderReminderTaskOptions();
  const latest = accounts
    .map((account) => account.last_refresh || 0)
    .sort((a, b) => b - a)[0];
  $("#lastRefresh").textContent = latest
    ? `最近刷新 ${new Date(latest * 1000).toLocaleString("zh-CN")}`
    : "尚未刷新";
}

async function refreshAccount(accountId) {
  toast("正在登录并同步任务…");
  try {
    const result = await api(`/api/accounts/${accountId}/refresh`, { method: "POST" });
    toast(`同步完成，发现 ${result.tasks} 项任务`);
  } catch (error) {
    toast(error.message, true);
  }
  await loadAll();
}

async function refreshAll() {
  const button = $("#refreshAllBtn");
  button.disabled = true;
  button.textContent = "正在同步…";
  try {
    const results = await api("/api/accounts/refresh", {
      method: "POST",
      body: JSON.stringify({}),
    });
    const success = results.filter((item) => item.ok).length;
    const failed = results.length - success;
    toast(`同步完成：${success} 个成功${failed ? `，${failed} 个失败` : ""}`, failed > 0);
  } catch (error) {
    toast(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = "刷新全部账号";
    await loadAll();
  }
}

function upcomingLocalValue(minutes = 60) {
  const date = new Date(Date.now() + minutes * 60 * 1000);
  const offset = date.getTimezoneOffset();
  return new Date(date.getTime() - offset * 60000).toISOString().slice(0, 16);
}

async function changeTaskStatus(taskId, status) {
  try {
    await api(`/api/tasks/${taskId}/status`, {
      method: "POST",
      body: JSON.stringify({ status }),
    });
    await loadAll();
  } catch (error) {
    toast(error.message, true);
  }
}

async function restoreCompletedTask(taskId) {
  try {
    await api(`/api/completed-tasks/${taskId}/restore`, { method: "POST" });
    toast("已重新标记为未完成");
    await loadAll();
    setView("tasks");
  } catch (error) {
    toast(error.message, true);
  }
}

async function lookupPigaiEssay() {
  const accountId = $("#pigaiAccount").value;
  const essayId = $("#pigaiEssayId").value.trim();
  const message = $("#pigaiLookupMessage");
  const button = $("#pigaiLookupBtn");
  if (!accountId) {
    toast("请先添加批改网账号", true);
    return;
  }
  if (!/^\d+$/.test(essayId)) {
    toast("作文号必须是数字", true);
    return;
  }
  button.disabled = true;
  button.textContent = "查询中…";
  message.textContent = "";
  try {
    const task = await api("/api/pigai/essay", {
      method: "POST",
      body: JSON.stringify({
        account_id: accountId,
        essay_id: essayId,
      }),
    });
    message.textContent = `已加入：${task.title}`;
    toast(`已找到作文：${task.title}`);
    $("#pigaiEssayId").value = "";
    await loadAll();
    renderTasks();
  } catch (error) {
    message.textContent = error.message;
    toast(error.message, true);
  } finally {
    button.disabled = false;
    button.textContent = "查询作文";
  }
}

async function openTask(taskId) {
  const task = state.tasks.find((item) => item.id === taskId);
  if (!task) return;
  const isMobile = /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent);
  try {
    const target = await api("/api/open", {
      method: "POST",
      body: JSON.stringify({
        account_id: task.account_id,
        task_id: taskId,
        mobile: isMobile,
      }),
    });
    if (target.kind === "mobile") {
      const links = [
        target.web_url ? `移动版：${target.web_url}` : "",
        target.android_url ? `Android：${target.android_url}` : "",
        target.ios_url ? `iOS：${target.ios_url}` : "",
      ].filter(Boolean);
      const choice = window.prompt(
        `手机端打开“${target.title || "任务"}”：\n1 复制移动版链接\n2 复制 Android 商店\n3 复制 iOS 商店\n\n${links.join(
          "\n"
        )}`,
        "1"
      );
      const selected =
        choice === "2"
          ? target.android_url
          : choice === "3"
          ? target.ios_url
          : target.web_url;
      if (selected) {
        window.location.href = selected;
      }
      return;
    }
    window.open(
      `/open/${encodeURIComponent(task.account_id)}/${encodeURIComponent(taskId)}`,
      "_blank",
      "noopener"
    );
  } catch (error) {
    toast(error.message, true);
  }
}

function openTaskReminderDialog(taskId) {
  const task = state.tasks.find((item) => item.id === taskId);
  if (!task) return;
  state.reminderTaskId = taskId;
  $("#dialogTaskName").textContent = `${task.title}${
    task.deadline ? ` · 截止 ${formatTime(task.deadline)}` : " · 无截止时间"
  }`;
  $("#dialogMode").value = task.deadline ? "countdown" : "absolute";
  $("#dialogCountdownField").classList.toggle("hidden", !task.deadline);
  $("#dialogAbsoluteField").classList.toggle("hidden", Boolean(task.deadline));
  $("#dialogTime").value = upcomingLocalValue(60);
  $("#dialogMessage").textContent = "";
  $("#reminderDialog").classList.remove("hidden");
}

function closeTaskReminderDialog() {
  state.reminderTaskId = null;
  $("#reminderDialog").classList.add("hidden");
}

function syncDialogMode() {
  const countdown = $("#dialogMode").value === "countdown";
  $("#dialogCountdownField").classList.toggle("hidden", !countdown);
  $("#dialogAbsoluteField").classList.toggle("hidden", countdown);
}

async function createTaskReminder() {
  if (!state.reminderTaskId) return;
  const mode = $("#dialogMode").value;
  const message = $("#dialogMessage");
  const button = $("#dialogConfirm");
  button.disabled = true;
  try {
    const payload =
      mode === "countdown"
        ? { mode, offset_minutes: Number($("#dialogOffset").value) }
        : { mode, remind_at: $("#dialogTime").value };
    await api(`/api/tasks/${state.reminderTaskId}/reminder`, {
      method: "POST",
      body: JSON.stringify(payload),
    });
    toast("已为该任务创建提醒");
    closeTaskReminderDialog();
    await loadAll();
    setView("reminders");
  } catch (error) {
    message.textContent = error.message;
  }
  button.disabled = false;
}

function wireEvents() {
  $$(".nav-item").forEach((item) => {
    item.addEventListener("click", () => setView(item.dataset.view));
  });
  $$("[data-view-jump]").forEach((item) => {
    item.addEventListener("click", () => setView(item.dataset.viewJump));
  });
  $("#refreshAllBtn").addEventListener("click", refreshAll);

  ["filterPlatform", "filterAccount", "filterStatus"].forEach((id) => {
    $(`#${id}`).addEventListener("change", renderTasks);
  });
  $("#taskSearch").addEventListener("input", renderTasks);
  $("#completedSearch").addEventListener("input", renderCompleted);
  $("#pigaiLookupBtn").addEventListener("click", lookupPigaiEssay);
  $("#pigaiEssayId").addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      lookupPigaiEssay();
    }
  });

  $("#accountForm").addEventListener("submit", async (event) => {
    event.preventDefault();
    const button = $("#addAccountBtn");
    const message = $("#accountFormMessage");
    button.disabled = true;
    button.textContent = "正在登录…";
    message.textContent = "";
    message.classList.remove("success");
    try {
      await api("/api/accounts", {
        method: "POST",
        body: JSON.stringify({
          platform: $("#accountPlatform").value,
          username: $("#accountUsername").value,
          password: $("#accountPassword").value,
          label: $("#accountLabel").value,
          refresh: $("#accountRefreshNow").checked,
        }),
      });
      $("#accountPassword").value = "";
      message.textContent = "账号已添加并同步";
      message.classList.add("success");
      toast("账号添加成功");
      await loadAll();
      setView("tasks");
    } catch (error) {
      message.textContent = error.message;
      toast(error.message, true);
    } finally {
      button.disabled = false;
      button.textContent = "添加并同步";
    }
  });

  $("#accountList").addEventListener("click", async (event) => {
    const refreshId = event.target.dataset.refresh;
    const deleteId = event.target.dataset.delete;
    if (refreshId) await refreshAccount(refreshId);
    if (deleteId && window.confirm("确定移除该账号及其本地任务缓存？")) {
      try {
        await api(`/api/accounts/${deleteId}`, { method: "DELETE" });
        await loadAll();
      } catch (error) {
        toast(error.message, true);
      }
    }
  });

  $("#taskTable").addEventListener("click", async (event) => {
    const remindId = event.target.dataset.remind;
    const statusId = event.target.dataset.status;
    const openId = event.target.dataset.open;
    if (openId) await openTask(openId);
    if (remindId) openTaskReminderDialog(remindId);
    if (statusId) await changeTaskStatus(statusId, event.target.dataset.next);
  });

  $("#completedTable").addEventListener("click", async (event) => {
    const openId = event.target.dataset.open;
    const restoreId = event.target.dataset.restore;
    if (openId) await openTask(openId);
    if (restoreId) await restoreCompletedTask(restoreId);
  });

  $("#reminderForm").addEventListener("submit", async (event) => {
    event.preventDefault();
    const message = $("#reminderFormMessage");
    message.textContent = "";
    try {
      if ($("#reminderMode").value === "countdown") {
        const taskId = $("#reminderTask").value;
        if (!taskId) throw new Error("请选择有关联任务");
        await api(`/api/tasks/${taskId}/reminder`, {
          method: "POST",
          body: JSON.stringify({
            mode: "countdown",
            offset_minutes: Number($("#reminderOffset").value),
          }),
        });
      } else {
        await api("/api/reminders", {
          method: "POST",
          body: JSON.stringify({
            title: $("#reminderTitle").value,
            remind_at: $("#reminderTime").value,
            repeat_rule: $("#reminderRepeat").value,
          }),
        });
      }
      $("#reminderTitle").value = "";
      message.textContent = "提醒已创建";
      message.classList.add("success");
      await loadAll();
    } catch (error) {
      message.textContent = error.message;
      toast(error.message, true);
    }
  });

  $("#reminderList").addEventListener("click", async (event) => {
    const toggleId = event.target.dataset.toggleReminder;
    const deleteId = event.target.dataset.deleteReminder;
    try {
      if (toggleId) {
        await api(`/api/reminders/${toggleId}`, {
          method: "PATCH",
          body: JSON.stringify({ enabled: event.target.dataset.enabled === "1" }),
        });
      }
      if (deleteId) {
        await api(`/api/reminders/${deleteId}`, { method: "DELETE" });
      }
      await loadAll();
    } catch (error) {
      toast(error.message, true);
    }
  });

  $("#reminderMode").addEventListener("change", () => {
    const countdown = $("#reminderMode").value === "countdown";
    $("#absoluteReminderField").classList.toggle("hidden", countdown);
    $("#sourceTaskField").classList.toggle("hidden", !countdown);
    $("#countdownField").classList.toggle("hidden", !countdown);
  });
  $("#dialogMode").addEventListener("change", syncDialogMode);
  $("#dialogClose").addEventListener("click", closeTaskReminderDialog);
  $("#dialogCancel").addEventListener("click", closeTaskReminderDialog);
  $("#dialogConfirm").addEventListener("click", createTaskReminder);
  $("#reminderDialog").addEventListener("click", (event) => {
    if (event.target === $("#reminderDialog")) closeTaskReminderDialog();
  });
}

(async function init() {
  wireEvents();
  $("#reminderTime").value = upcomingLocalValue(60);
  try {
    await loadAll();
  } catch (error) {
    toast(error.message, true);
  }
})();

