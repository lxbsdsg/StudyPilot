import { describe, expect, it, vi } from "vitest";
import type { Task } from "../types";
import {
  compareTasks,
  deadlineDescription,
  deadlineTone,
  filterAndSortTasks,
  formatDateTime,
  formatTimestamp,
  repeatRuleLabels,
  toLocalInputValue,
} from "./format";

function task(overrides: Partial<Task>): Task {
  return {
    id: "task-1",
    account_id: "account-1",
    platform: "unipus",
    course_id: "course-1",
    course_name: "大学英语",
    title: "Unit 1",
    task_id: "1",
    status: "pending",
    deadline: null,
    url: "https://example.test",
    detail: "",
    source: "",
    ...overrides,
  };
}

describe("format helpers", () => {
  it("formats local dates and numeric timestamps", () => {
    expect(formatDateTime("2026-11-16T09:30")).toContain("11/16");
    expect(formatDateTime("2026-11-16 09:30")).toContain("11/16");
    expect(formatDateTime(null)).toBe("未设置");
    expect(formatDateTime("invalid")).toBe("未设置");
    expect(formatTimestamp(0)).toBe("尚未同步");
    expect(formatTimestamp(1_700_000_000)).not.toBe("尚未同步");
  });

  it("builds a datetime-local value in local time", () => {
    const date = new Date("2026-11-16T09:30:00");
    expect(toLocalInputValue(date)).toBe("2026-11-16T09:30");
  });

  it("classifies deadline urgency", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-16T09:00:00"));
    expect(deadlineTone(null)).toBe("neutral");
    expect(deadlineTone("2026-11-16T08:59")).toBe("danger");
    expect(deadlineTone("2026-11-17T09:00")).toBe("warning");
    expect(deadlineTone("2026-11-30T09:00")).toBe("neutral");
    expect(deadlineDescription(null)).toBe("无截止时间");
    vi.useRealTimers();
  });

  it("has a label for every repeat rule", () => {
    expect(repeatRuleLabels.daily).toBe("每天");
    expect(repeatRuleLabels.weekdays).toBe("每个工作日");
  });
});

describe("task filtering and sorting", () => {
  const tasks = [
    task({
      id: "later",
      title: "写作作业",
      deadline: "2026-11-30T12:00",
      status: "pending",
      updated_at: 10,
    }),
    task({
      id: "urgent",
      title: "听力练习",
      deadline: "2026-11-17T12:00",
      platform: "pigai",
      account_id: "account-2",
      status: "pending",
      updated_at: 20,
    }),
    task({
      id: "no-deadline",
      title: "复习单元",
      detail: "期末范围",
      status: "ignored",
      updated_at: 30,
    }),
  ];

  it("filters by status, platform, account, and query", () => {
    expect(
      filterAndSortTasks(tasks, { status: "pending", platform: "pigai" }).map(
        (item) => item.id,
      ),
    ).toEqual(["urgent"]);
    expect(
      filterAndSortTasks(tasks, { status: "all", accountId: "account-1" }).map(
        (item) => item.id,
      ),
    ).toEqual(["later", "no-deadline"]);
    expect(
      filterAndSortTasks(tasks, { status: "all", query: "期末" }).map((item) => item.id),
    ).toEqual(["no-deadline"]);
  });

  it("sorts deadlines first, then tasks without deadlines", () => {
    expect(
      filterAndSortTasks(tasks, { status: "all", sort: "deadline" }).map((item) => item.id),
    ).toEqual(["urgent", "later", "no-deadline"]);
    expect(
      filterAndSortTasks(tasks, { status: "all", sort: "updated" }).map((item) => item.id),
    ).toEqual(["no-deadline", "urgent", "later"]);
  });

  it("compares missing update timestamps safely", () => {
    expect(compareTasks(task({}), task({ updated_at: 1 }))).toBe(1);
    expect(compareTasks(task({ updated_at: 2 }), task({}))).toBe(-2);
  });
});
