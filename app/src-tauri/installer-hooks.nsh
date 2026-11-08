; Haul's NSIS installer hooks (bundle.windows.nsis.installerHooks in tauri.conf.json).
;
; A real uninstall removes Haul's Start with Windows entry, so Windows doesn't try to start a
; program that is gone (Tauri's own uninstaller does the same; this keeps it explicit). An update
; (/UPDATE) keeps it.
;
; Task Manager's own "disabled" mark for Haul is left alone on purpose. An upgrade that
; uninstalls first runs this uninstaller too, and the new Haul writes its Run value back at
; start; keeping the mark means a startup the user disabled in Task Manager stays disabled.
;
; App data in %APPDATA%\dev.haul.desktop stays unless the user ticks the uninstaller's own
; "delete app data" box.

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Haul"
  ${EndIf}
!macroend
