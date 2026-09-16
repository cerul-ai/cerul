# Terminal demo for the README, recorded with charmbracelet/vhs.
#
#   brew install vhs          # or: go install github.com/charmbracelet/vhs@latest
#   vhs scripts/demo.tape     # writes docs/assets/cerul-demo.gif
#
# Record against a real indexed video so the result cards and match
# percentages are genuine. Set CERUL_DEMO_VIDEO to the clip you want to use;
# indexing it costs a few Gemini API calls.
#
# Adding the GIF to the README also needs two one-line changes, because the
# repository checks which binary assets ship in the source package:
#   - Cargo.toml        include: add "/docs/assets/*.gif"
#   - scripts/check-docs.py  asset allowlist: add docs/assets/cerul-demo.gif

Output docs/assets/cerul-demo.gif

Set Shell zsh
Set FontSize 16
Set Width 1100
Set Height 640
Set Padding 24
Set Theme "Catppuccin Latte"
Set TypingSpeed 60ms
Set PlaybackSpeed 1.0

Hide
Type "clear" Enter
Show

Sleep 500ms

Type "cerul index ./demo.mp4"
Sleep 400ms
Enter
Sleep 12s

Type "cerul search 'A person puts a cup on the table'"
Sleep 400ms
Enter
Sleep 8s

Type "cerul search 'A person puts a cup on the table' --save ./clips"
Sleep 400ms
Enter
Sleep 6s

Sleep 2s
