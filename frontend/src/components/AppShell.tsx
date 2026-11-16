import {
  BellRing,
  BookOpenCheck,
  CheckCircle2,
  ChevronDown,
  CloudCog,
  GraduationCap,
  LayoutDashboard,
  Menu,
  Plus,
  RefreshCw,
  Sparkles,
  UserRound,
  X,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import type { Platform, WorkspaceData } from "../types";
import { formatTimestamp } from "../lib/format";
import { Button, IconButton, Tag } from "./Primitives";

export type ViewId = "overview" | "tasks" | "completed" | "accounts" | "reminders";

const pageCopy: Record<ViewId, [string, string]> = {
  overview: ["学习总览", "把握所有平台的下一步"],
  tasks: ["学习任务", "按截止时间安排你的学习"],
  completed: ["已完成", "回看近七天的完成记录"],
  accounts: ["账号管理", "连接课程平台与本地数据"],
  reminders: ["提醒计划", "在重要节点前留下余量"],
};

const navItems = [
  { id: "overview", label: "总览", icon: LayoutDashboard },
  { id: "tasks", label: "学习任务", icon: BookOpenCheck },
  { id: "completed", label: "已完成", icon: CheckCircle2 },
  { id: "accounts", label: "账号", icon: UserRound },
  { id: "reminders", label: "提醒", icon: BellRing },
] as const;

function NavButton({
  id,
  label,
  icon: Icon,
  active,
  count,
  onClick,
}: {
  id: ViewId;
  label: string;
  icon: typeof LayoutDashboard;
  active: boolean;
  count?: number;
  onClick: (id: ViewId) => void;
}) {
  return (
    <button
      className={`nav-item ${active ? "active" : ""}`}
      data-view={id}
      onClick={() => onClick(id)}
      aria-current={active ? "page" : undefined}
    >
      <Icon size={18} strokeWidth={1.8} />
      <span>{label}</span>
      {count !== undefined && count > 0 && <b className="nav-count">{count}</b>}
    </button>
  );
}

function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <div className="brand">
      <span className="brand-mark" aria-hidden="true">
        <GraduationCap size={21} />
      </span>
      <span className="brand-copy">
        <strong>StudyPilot</strong>
        {!compact && <small>学习任务中心</small>}
      </span>
    </div>
  );
}

export function AppShell({
  view,
  onView,
  data,
  syncing,
  onSyncAll,
  onAddAccount,
  onRefresh,
  children,
}: {
  view: ViewId;
  onView: (view: ViewId) => void;
  data: WorkspaceData;
  syncing: boolean;
  onSyncAll: () => void;
  onAddAccount: () => void;
  onRefresh: () => void;
  children: ReactNode;
}) {
  const [mobileNav, setMobileNav] = useState(false);
  const [accountMenu, setAccountMenu] = useState(false);
  const [title, subtitle] = pageCopy[view];
  const latestRefresh = Math.max(0, ...data.accounts.map((account) => account.last_refresh));
  const accountSummary = data.accounts
    .slice(0, 4)
    .map((account) => (
      <div className="account-menu-row" key={account.id}>
        <span className={`platform-symbol platform-${account.platform}`}>
          {platformInitial(account.platform, data.platforms)}
        </span>
        <span>
          <strong>{account.label || account.display_name || account.username}</strong>
          <small>{platformLabel(account.platform, data.platforms)}</small>
        </span>
        <Tag tone={account.status === "ready" ? "success" : "danger"}>
          {account.status === "ready" ? "正常" : "异常"}
        </Tag>
      </div>
    ));
  return (
    <div className="app-shell">
      <aside className={`sidebar ${mobileNav ? "mobile-open" : ""}`}>
        <div className="sidebar-top">
          <Brand />
          <IconButton
            label="关闭导航"
            className="mobile-close"
            onClick={() => setMobileNav(false)}
          >
            <X size={18} />
          </IconButton>
        </div>
        <nav className="nav" aria-label="主导航">
          <span className="nav-eyebrow">工作区</span>
          {navItems.map(({ id, label, icon }) => (
            <NavButton
              key={id}
              id={id}
              label={label}
              icon={icon}
              active={view === id}
              count={
                id === "tasks"
                  ? data.dashboard.pending
                  : id === "completed"
                    ? data.completed.length
                    : id === "reminders"
                      ? data.reminders.filter((reminder) => reminder.enabled).length
                      : undefined
              }
              onClick={(next) => {
                onView(next);
                setMobileNav(false);
              }}
            />
          ))}
        </nav>
        <div className="sidebar-bottom">
          <section className="sync-card">
            <span className="sync-card-icon">
              <CloudCog size={18} />
            </span>
            <strong>本地学习数据</strong>
            <small>
              {syncing ? "正在读取最新学习状态" : `最近同步 ${formatTimestamp(latestRefresh)}`}
            </small>
            <Button
              variant="ghost"
              size="small"
              icon={<RefreshCw size={15} className={syncing ? "spin" : ""} />}
              onClick={onSyncAll}
              disabled={syncing || data.accounts.length === 0}
            >
              同步全部
            </Button>
          </section>
          <div className="sidebar-help">
            <Sparkles size={15} />
            <span>专注眼下，逐项推进</span>
          </div>
        </div>
      </aside>
      {mobileNav && (
        <button
          className="nav-scrim"
          aria-label="关闭导航"
          onClick={() => setMobileNav(false)}
        />
      )}
      <main className="main">
        <header className="topbar">
          <div className="topbar-title">
            <IconButton
              label="打开导航"
              className="mobile-menu"
              onClick={() => setMobileNav(true)}
            >
              <Menu size={20} />
            </IconButton>
            <div className="mobile-brand">
              <Brand compact />
            </div>
            <div className="page-heading">
              <h1>{title}</h1>
              <p>{subtitle}</p>
            </div>
          </div>
          <div className="top-actions">
            <span className="health-status">
              <i />
              本地服务正常
            </span>
            <div className="account-menu-wrap">
              <button
                className="account-switch"
                aria-label="切换账号"
                onClick={() => setAccountMenu(!accountMenu)}
                disabled={data.accounts.length === 0}
              >
                <AccountAvatar count={data.accounts.length} />
                <ChevronDown size={15} />
              </button>
              {accountMenu && (
                <div className="account-menu">
                  <div className="account-menu-head">
                    <span>已连接账号</span>
                    <IconButton label="关闭账号列表" onClick={() => setAccountMenu(false)}>
                      <X size={15} />
                    </IconButton>
                  </div>
                  {accountSummary}
                  <Button
                    variant="ghost"
                    size="small"
                    icon={<RefreshCw size={14} />}
                    onClick={() => {
                      setAccountMenu(false);
                      onRefresh();
                    }}
                  >
                    管理全部账号
                  </Button>
                </div>
              )}
            </div>
            <Button
              className="add-account-button"
              variant="primary"
              icon={<Plus size={16} />}
              onClick={onAddAccount}
            >
              添加账号
            </Button>
          </div>
        </header>
        <div className="view-content" key={view}>
          {children}
        </div>
        <footer className="app-footer">
          <span>StudyPilot</span>
          <span className="footer-dot" />
          <span>数据仅保存在本机</span>
        </footer>
      </main>
      <nav className="mobile-nav" aria-label="移动端导航">
        {navItems.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            className={view === id ? "active" : ""}
            onClick={() => onView(id)}
            aria-current={view === id ? "page" : undefined}
          >
            <Icon size={19} />
            <span>{label}</span>
            {id === "tasks" && data.dashboard.pending > 0 && (
              <em className="mobile-badge">{data.dashboard.pending}</em>
            )}
          </button>
        ))}
      </nav>
    </div>
  );
}

function AccountAvatar({ count }: { count: number }) {
  return (
    <span className="account-avatar">
      {count >= 10 ? "9+" : count > 0 ? count : <UserRound size={16} />}
    </span>
  );
}

export function platformLabel(platform: string, platforms: Platform[]) {
  return platforms.find((item) => item.id === platform)?.label || platform;
}

export function platformInitial(platform: string, platforms: Platform[]) {
  return platformLabel(platform, platforms).slice(0, 1);
}
