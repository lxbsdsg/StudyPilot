from studypilot.open_targets import (
    desktop_url,
    is_mobile_user_agent,
    mobile_targets,
)


def test_mobile_user_agent_detection():
    assert is_mobile_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)")
    assert is_mobile_user_agent("Mozilla/5.0 (Linux; Android 14)")
    assert not is_mobile_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")


def test_desktop_url_prefers_task_url():
    assert desktop_url("pigai", "https://www.pigai.org/essay/1") == (
        "https://www.pigai.org/essay/1"
    )
    assert desktop_url("pigai") == "https://www.pigai.org/"


def test_pigai_mobile_targets_include_apps():
    target = mobile_targets("pigai", "https://www.pigai.org/essay/1")
    assert target["kind"] == "mobile"
    assert target["web_url"] == "https://www.pigai.org/zt/mobile/"
    assert "sj.qq.com" in target["android_url"]
    assert "itunes.apple.com" in target["ios_url"]


def test_other_platform_mobile_targets():
    target = mobile_targets("chaoxing", "https://mooc1.chaoxing.com/task")
    assert target["web_url"] == "https://mooc1.chaoxing.com/"
    assert target["desktop_url"] == "https://mooc1.chaoxing.com/task"

