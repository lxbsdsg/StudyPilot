export type PlatformId = "unipus" | "welearn" | "chaoxing" | "pigai";
export type TaskStatus = "pending" | "completed" | "ignored";
export type RepeatRule = "once" | "daily" | "weekdays" | "weekly";
export type ExecutionMode = "native-rust" | "python-compatibility" | "unsupported";

export interface Platform {
  id: PlatformId;
  label: string;
  execution_mode?: ExecutionMode;
  check?: {
    status?: string;
    note?: string;
    checked_at?: number;
  };
}

export interface Account {
  id: string;
  platform: PlatformId;
  username: string;
  label: string;
  display_name: string;
  school: string;
  status: string;
  last_error: string;
  last_refresh: number;
  created_at: number;
}

export interface Course {
  account_id: string;
  platform: PlatformId;
  id: string;
  name: string;
  progress: number;
}

export interface Task {
  id: string;
  account_id: string;
  platform: PlatformId;
  course_id: string;
  course_name: string;
  title: string;
  task_id: string;
  status: TaskStatus;
  deadline: string | null;
  url: string;
  detail: string;
  source: string;
  updated_at?: number;
}

export interface CompletedTask extends Task {
  completion_id: string;
  completed_at: number;
}

export interface Reminder {
  id: string;
  account_id: string | null;
  task_id: string | null;
  title: string;
  remind_at: string;
  repeat_rule: RepeatRule;
  enabled: boolean;
  last_triggered: number;
  task_title?: string | null;
  platform?: PlatformId | null;
  account_label?: string | null;
}

export interface PlatformSummary {
  platform: PlatformId;
  label: string;
  accounts: number;
  pending: number;
  errors: number;
}

export interface Dashboard {
  accounts: number;
  courses: number;
  pending: number;
  due_soon: number;
  reminders: number;
  platforms: PlatformSummary[];
  platform_checks: Record<string, { status: string; note: string; checked_at: number }>;
  next_tasks: Task[];
}

export interface RefreshResult {
  account_id?: string;
  platform?: PlatformId;
  ok: boolean;
  tasks?: number;
  courses?: number;
  warning?: string;
  error?: string;
}

export interface AccountInput {
  platform: PlatformId;
  username: string;
  password: string;
  label: string;
}

export interface WorkspaceData {
  platforms: Platform[];
  accounts: Account[];
  courses: Course[];
  tasks: Task[];
  completed: CompletedTask[];
  reminders: Reminder[];
  dashboard: Dashboard;
}
