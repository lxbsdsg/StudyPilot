from studypilot.notifications import _powershell_literal


def test_powershell_text_is_safely_quoted():
    assert _powershell_literal("Today's task") == "'Today''s task'"
    assert _powershell_literal("$(Remove-Item C:\\data)") == (
        "'$(Remove-Item C:\\data)'"
    )
