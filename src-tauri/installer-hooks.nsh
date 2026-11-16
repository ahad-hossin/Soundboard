; Offers to install yt-dlp and ffmpeg (for "From link") with winget after Soundboard installs.
; Skipped on silent installs, when winget is missing, and when both tools are already found.
!macro NSIS_HOOK_POSTINSTALL
  IfSilent sb_link_done
  nsExec::Exec 'cmd /c where winget'
  Pop $0
  StrCmp $0 0 0 sb_link_done
  nsExec::Exec 'cmd /c where yt-dlp && where ffmpeg'
  Pop $0
  StrCmp $0 0 sb_link_done
  MessageBox MB_YESNO|MB_ICONQUESTION "Install yt-dlp and ffmpeg so Soundboard can add sounds from links (YouTube, Instagram, Facebook and more)?$\r$\n$\r$\nThis uses winget and downloads about 100 MB. You can skip this and install them later." IDNO sb_link_done
  ExecWait 'winget install -e --id yt-dlp.yt-dlp --accept-source-agreements --accept-package-agreements'
  ExecWait 'winget install -e --id Gyan.FFmpeg --accept-source-agreements --accept-package-agreements'
  sb_link_done:
!macroend
