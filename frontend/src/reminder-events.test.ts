import { describe, expect, it } from "vitest";
import { isReminderPayload, reminderToastMessage } from "./reminder-events";
import type { Reminder } from "./types";

const reminder: Reminder = {
  id: "reminder-1",
  account_id: null,
  task_id: null,
  title: "复习 Unit 2",
  remind_at: "2026-11-16T20:00",
  repeat_rule: "once",
  enabled: false,
  last_triggered: 1_700_000_000,
};

describe("desktop reminder events", () => {
  it("validates and formats the event payload", () => {
    expect(isReminderPayload(reminder)).toBe(true);
    expect(reminderToastMessage(reminder)).toBe("复习 Unit 2");
    expect(isReminderPayload({ title: "缺字段" })).toBe(false);
  });

  it("falls back for a blank title", () => {
    expect(reminderToastMessage({ ...reminder, title: " " })).toBe("学习提醒已到时间");
  });
});
