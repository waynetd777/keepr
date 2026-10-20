; Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
; See LICENSE for the full text.
; SPDX-License-Identifier: GPL-3.0-or-later

; Uninstalling removes Keepr Helper if it was installed, and takes Open at Login's entry out of
; the user's Run key (login_win.rs), so Windows doesn't try to start a program that's gone. The uninstaller's "Delete the application data" box
; clears %APPDATA%\<identifier>, but Keepr keeps its settings and history in %APPDATA%\Keepr
; (config.rs data_dir). With the box ticked, and not for an update, that goes too, with the update
; installers the updater left in Temp. The backups themselves, at their destinations, always stay.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; Keepr Helper, the service that makes still copies (helper_win.rs), lives in Program Files,
    ; outside this per-user install: its own copy removes it, with Windows' administrator prompt.
    ${If} ${FileExists} "$PROGRAMFILES64\Keepr Helper\KeeprHelper.exe"
      ExecShellWait "runas" "$PROGRAMFILES64\Keepr Helper\KeeprHelper.exe" "--helper-uninstall" SW_HIDE
    ${EndIf}
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
  ${EndIf}
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\${PRODUCTNAME}"
    FindFirst $0 $1 "$TEMP\${PRODUCTNAME}-*-updater-*"
    ${DoWhile} $1 != ""
      RmDir /r "$TEMP\$1"
      FindNext $0 $1
    ${Loop}
    FindClose $0
  ${EndIf}
!macroend
