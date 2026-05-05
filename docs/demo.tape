# Orch8 launch demo — recorded with VHS (https://github.com/charmbracelet/vhs).
# Regenerate from this directory: `vhs demo.tape`
# Output lands at docs/demo.gif (referenced from the top of README.md).

Output demo.gif

Set FontSize 18
Set Width 1280
Set Height 720
Set Theme "Dracula"
Set Padding 24
Set TypingSpeed 55ms
Set Shell "zsh"

# Hidden setup: scratch dir + env override. The brewed `orch8` binary may
# still ship the old `sqlite://orch8.db` template; the env var bypasses it.
# After the next release, remove the export and re-record.
Hide
Type "cd $(mktemp -d) && export ORCH8_DATABASE_URL='sqlite:orch8.db?mode=rwc' && clear"
Enter
Sleep 500ms
Show

Type "orch8 init demo"
Sleep 200ms
Enter
Sleep 1800ms

Type "cd demo"
Sleep 200ms
Enter
Sleep 400ms

Type "orch8-server --config orch8.toml"
Sleep 200ms
Enter
Sleep 4500ms
