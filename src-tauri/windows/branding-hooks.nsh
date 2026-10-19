; OrbitPal display-name migration. Installed data and registry identities stay fixed.
; The template restores the existing directory from the legacy registry key.
; Only shortcuts with our exact old/new executable target are touched.

!macro NSIS_HOOK_PREINSTALL
  ; Recognize the old process before replacing its executable during manual upgrades.
  ${If} ${FileExists} "$INSTDIR\streamer-ai-cohost.exe"
    !insertmacro CheckIfAppIsRunning "$INSTDIR\streamer-ai-cohost.exe" "Streamer AI Cohost"
  ${EndIf}
!macroend

!macro ORBITPAL_RETARGET_LEGACY_SHORTCUT SHORTCUT NEW_SHORTCUT
  !insertmacro IsShortcutTarget "${SHORTCUT}" "$INSTDIR\streamer-ai-cohost.exe"
  Pop $0
  ${If} $0 = 1
    ; Existing pins can still resolve the old shortcut location.
    !insertmacro SetShortcutTarget "${SHORTCUT}" "$INSTDIR\${MAINBINARYNAME}.exe"
    ${IfNot} ${FileExists} "${NEW_SHORTCUT}"
      CreateShortcut "${NEW_SHORTCUT}" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "${NEW_SHORTCUT}"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro ORBITPAL_RETARGET_LEGACY_SHORTCUT "$DESKTOP\Streamer AI Cohost.lnk" "$DESKTOP\OrbitPal.lnk"
  !insertmacro ORBITPAL_RETARGET_LEGACY_SHORTCUT "$SMPROGRAMS\Streamer AI Cohost.lnk" "$SMPROGRAMS\OrbitPal.lnk"
  !insertmacro ORBITPAL_RETARGET_LEGACY_SHORTCUT "$SMPROGRAMS\Streamer AI Cohost\Streamer AI Cohost.lnk" "$SMPROGRAMS\OrbitPal.lnk"
!macroend

!macro ORBITPAL_REMOVE_LEGACY_SHORTCUT SHORTCUT
  !insertmacro IsShortcutTarget "${SHORTCUT}" "$INSTDIR\${MAINBINARYNAME}.exe"
  Pop $0
  ${If} $0 = 1
    Delete "${SHORTCUT}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro ORBITPAL_REMOVE_LEGACY_SHORTCUT "$DESKTOP\Streamer AI Cohost.lnk"
    !insertmacro ORBITPAL_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\Streamer AI Cohost.lnk"
    !insertmacro ORBITPAL_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\Streamer AI Cohost\Streamer AI Cohost.lnk"
    RMDir "$SMPROGRAMS\Streamer AI Cohost"
  ${EndIf}
!macroend
