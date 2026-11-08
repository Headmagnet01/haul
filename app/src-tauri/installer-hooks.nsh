; Haul's NSIS installer hooks (bundle.windows.nsis.installerHooks in tauri.conf.json).
;
; Uninstalling removes Haul's Start with Windows entry, and the mark Task Manager keeps
; for it, so Windows doesn't try to start a program that is gone. App data in
; %APPDATA%\dev.haul.desktop stays unless the user ticks the uninstaller's own
; "delete app data" box.

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Haul"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Haul"
!macroend
