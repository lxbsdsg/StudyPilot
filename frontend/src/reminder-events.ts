import type { Reminder } from "./types";

export const REMINDER_EVENT = "reminder.triggered";

export function reminderToastMessage(reminder: Reminder): string {
  return reminder.title.trim() || "学习提醒已到时间";
}

export function isReminderPayload(value: unknown): value is Reminder {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<Reminder>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.title === "string" &&
    typeof candidate.remind_at === "string" &&
    typeof candidate.repeat_rule === "string" &&
    typeof candidate.enabled === "boolean"
  );
}
