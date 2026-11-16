import {
  CloudCog,
  Eye,
  EyeOff,
  KeyRound,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
  UserRoundPlus,
  Wifi,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { Account, AccountInput, Platform, PlatformId, WorkspaceData } from "../types";
import { accountName, formatTimestamp, platformTone } from "../lib/format";
import {
  Button,
  EmptyState,
  Field,
  IconButton,
  StatusMark,
  Tag,
} from "../components/Primitives";
import { platformInitial, platformLabel } from "../components/AppShell";

export interface AccountFormValue {
  platform: PlatformId;
  username: string;
  password: string;
  label: string;
}

const platformNotes: Record<PlatformId, string> = {
  unipus: "输入 U校园登录账号与密码，用于读取课程和学习资源。",
  welearn: "登录流程与学校 SSO 有关，请使用平时登录 WELearn 的账号。",
  chaoxing: "使用学习通手机号或学号登录，课程与作业会汇总到任务列表。",
  pigai: "登录后可同步提交中的作文，也可通过作文号直接查询。",
};

export function AccountsView({
  data,
  formOpen,
  onFormOpenChange,
  onAdd,
  onRefresh,
  onRefreshAll,
  onDelete,
  syncing,
  pendingAccountId,
}: {
  data: WorkspaceData;
  formOpen: boolean;
  onFormOpenChange: (open: boolean) => void;
  onAdd: (input: AccountInput) => Promise<void>;
  onRefresh: (account: Account) => Promise<void>;
  onRefreshAll: () => void;
  onDelete: (account: Account) => Promise<void>;
  syncing: boolean;
  pendingAccountId: string | null;
}) {
  return (
    <div className="accounts-view">
      <section className="list-intro account-intro">
        <div>
          <span className="eyebrow">CONNECTIONS</span>
          <h2>连接你的学习平台</h2>
          <p>
            已连接 {data.accounts.length} 个账号。登录凭据加密保存在本机，数据只用于汇总学习任务。
          </p>
        </div>
        <div className="intro-actions">
          <Button
            variant="secondary"
            icon={<RefreshCw size={16} className={syncing ? "spin" : ""} />}
            onClick={onRefreshAll}
            disabled={syncing || data.accounts.length === 0}
          >
            同步全部
          </Button>
          <Button
            variant="primary"
            icon={<Plus size={16} />}
            onClick={() => onFormOpenChange(!formOpen)}
          >
            添加账号
          </Button>
        </div>
      </section>

      {formOpen && (
        <AccountForm
          platforms={data.platforms}
          onAdd={onAdd}
          onCancel={() => onFormOpenChange(false)}
        />
      )}

      <section className="surface accounts-board">
        <header className="section-header">
          <div>
            <span className="section-kicker">ACCOUNT STATUS</span>
            <h3>已连接的账号</h3>
          </div>
          {data.accounts.length > 0 && (
            <span className="section-note">
              <Wifi size={15} />
              {data.accounts.filter((account) => account.status === "ready").length} 个状态正常
            </span>
          )}
        </header>
        {data.accounts.length ? (
          <div className="account-list">
            {data.accounts.map((account) => (
              <AccountRow
                key={account.id}
                account={account}
                platforms={data.platforms}
                onRefresh={() => onRefresh(account)}
                onDelete={() => onDelete(account)}
                refreshing={pendingAccountId === account.id || syncing}
              />
            ))}
          </div>
        ) : (
          <EmptyState
            icon={<UserRoundPlus size={29} />}
            title="还没有连接学习账号"
            description="添加一个平台账号后，课程、作业和截止时间会汇总到这里。"
            action={
              <Button
                variant="primary"
                size="small"
                icon={<Plus size={15} />}
                onClick={() => onFormOpenChange(true)}
              >
                添加第一个账号
              </Button>
            }
          />
        )}
      </section>
    </div>
  );
}

function AccountForm({
  platforms,
  onAdd,
  onCancel,
}: {
  platforms: Platform[];
  onAdd: (input: AccountInput) => Promise<void>;
  onCancel: () => void;
}) {
  const initialPlatform = platforms[0]?.id ?? "unipus";
  const [platform, setPlatform] = useState<PlatformId>(initialPlatform);
  const selectedPlatform = platforms.find((item) => item.id === platform);
  const [label, setLabel] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!platforms.some((item) => item.id === platform) && platforms[0]) {
      setPlatform(platforms[0].id);
    }
  }, [platform, platforms]);

  const submit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    if (!username.trim() || !password) {
      setError("请填写登录账号和密码");
      return;
    }
    setBusy(true);
    try {
      await onAdd({
        platform,
        username: username.trim(),
        password,
        label: label.trim(),
      });
      setUsername("");
      setPassword("");
      setLabel("");
      onCancel();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "账号添加失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="surface account-form">
      <header className="section-header">
        <div>
          <span className="section-kicker">NEW CONNECTION</span>
          <h3>添加学习账号</h3>
        </div>
        <span className="security-note">
          <ShieldCheck size={15} />
          凭据仅保存在本机
        </span>
      </header>
      <div className="platform-picker" role="group" aria-label="选择学习平台">
        {platforms.map((item) => (
          <button
            type="button"
            key={item.id}
            className={`platform-choice ${platform === item.id ? "selected" : ""}`}
            onClick={() => setPlatform(item.id)}
            aria-pressed={platform === item.id}
          >
            <span className={`platform-symbol platform-${item.id}`}>
              {platformInitial(item.id, platforms)}
            </span>
            <span>
              <strong>{item.label}</strong>
              <small>{platformLabel(item.id, platforms)}</small>
            </span>
          </button>
        ))}
      </div>
      <div className="platform-note">
        <CloudCog size={18} />
        <div>
          <p>{platformNotes[platform]}</p>
          {selectedPlatform?.execution_mode === "python-compatibility" && (
            <small>兼容模式：由 Rust 托管 Python 抓取，结果经校验后统一写入本地数据库。</small>
          )}
        </div>
      </div>
      <form className="account-form-grid" onSubmit={submit}>
        <Field label="账号备注" hint="可选，方便区分多个账号">
          <input
            value={label}
            onChange={(event) => setLabel(event.target.value)}
            placeholder="例如：主账号"
            autoComplete="off"
          />
        </Field>
        <Field label="登录账号">
          <input
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            placeholder="手机号、学号或用户名"
            autoComplete="username"
            required
          />
        </Field>
        <Field label="登录密码">
          <div className="password-input">
            <input
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="请输入平台密码"
              autoComplete="current-password"
              required
            />
            <IconButton
              type="button"
              label={showPassword ? "隐藏密码" : "显示密码"}
              onClick={() => setShowPassword(!showPassword)}
            >
              {showPassword ? <EyeOff size={17} /> : <Eye size={17} />}
            </IconButton>
          </div>
        </Field>
        <div className="account-form-actions">
          <Button type="button" variant="secondary" onClick={onCancel} disabled={busy}>
            取消
          </Button>
          <Button
            type="submit"
            variant="primary"
            icon={<KeyRound size={16} />}
            disabled={busy}
          >
            {busy ? "正在连接并同步…" : "添加并同步"}
          </Button>
        </div>
        {error && <p className="form-error account-form-error">{error}</p>}
      </form>
    </section>
  );
}

function AccountRow({
  account,
  platforms,
  onRefresh,
  onDelete,
  refreshing,
}: {
  account: Account;
  platforms: Platform[];
  onRefresh: () => Promise<void>;
  onDelete: () => Promise<void>;
  refreshing: boolean;
}) {
  const [deleteBusy, setDeleteBusy] = useState(false);
  const executionMode = platforms.find((item) => item.id === account.platform)?.execution_mode;
  return (
    <article className="account-row">
      <div className={`platform-icon platform-${platformTone(account.platform)}`}>
        {platformInitial(account.platform, platforms)}
      </div>
      <div className="account-main">
        <div className="account-name-line">
          <strong>{accountName(account)}</strong>
          <Tag tone="neutral">{platformLabel(account.platform, platforms)}</Tag>
          {executionMode === "python-compatibility" && <Tag tone="warning">兼容模式</Tag>}
        </div>
        <small>
          {account.username}
          {account.school ? ` · ${account.school}` : ""}
        </small>
      </div>
      <div className="account-state">
        <StatusMark status={account.status} />
        <small>{account.last_refresh ? formatTimestamp(account.last_refresh) : "尚未完成同步"}</small>
      </div>
      {account.last_error ? (
        <div className="account-error" title={account.last_error}>
          {account.last_error}
        </div>
      ) : (
        <div className="account-ok">
          <ShieldCheck size={15} />
          凭据已加密
        </div>
      )}
      <div className="account-actions">
        <IconButton label="同步此账号" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw size={17} className={refreshing ? "spin" : ""} />
        </IconButton>
        <IconButton
          label="删除账号"
          className="danger-icon"
          disabled={deleteBusy}
          onClick={async () => {
            setDeleteBusy(true);
            try {
              await onDelete();
            } finally {
              setDeleteBusy(false);
            }
          }}
        >
          <Trash2 size={17} />
        </IconButton>
      </div>
    </article>
  );
}
