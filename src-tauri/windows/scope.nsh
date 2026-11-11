; Included by the pinned Tauri template. Definitions expand after its standard UI.
; Keep the app and current-user setup unelevated; relaunch only machine operations.
; ShellExecuteEx lets the original process wait and return the elevated child's
; actual exit code, including when invoked by the passive updater.
!macro DBSAUR_ELEVATE PREFIX TRAILER
Function ${PREFIX}ScopeElevate
  ${${PREFIX}GetParameters} $ScopeArguments
  StrCpy $ScopeArguments '/AllUsers $ScopeArguments ${TRAILER}'
  !if ${NSIS_PTR_SIZE} > 4
    !define /ReDef DBSAUR_SHELLEXECUTEINFO_SIZE 112
  !else
    !define /ReDef DBSAUR_SHELLEXECUTEINFO_SIZE 60
  !endif
  System::Call '*(i ${DBSAUR_SHELLEXECUTEINFO_SIZE}, i 0x40, p $HWNDPARENT, t "runas", t "$EXEPATH", t "$ScopeArguments", p 0, i 1, p 0, p 0, p 0, p 0, i 0, p 0, p 0) p.r0'
  System::Call 'shell32::ShellExecuteExW(p r0) i.r1'
  ${If} $1 = 0
    System::Free $0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Administrator permission is required for an all-users installation. Choose current user to install without elevation."
    SetErrors
    Return
  ${EndIf}
  System::Call '*$0(i, i, p, p, p, p, p, i, p, p, p, p, i, p, p.r1)'
  System::Free $0
  HideWindow
  System::Call 'kernel32::WaitForSingleObject(p r1, i -1)'
  System::Call 'kernel32::GetExitCodeProcess(p r1, *i .r2)'
  System::Call 'kernel32::CloseHandle(p r1)'
  SetErrorLevel $2
  Quit
FunctionEnd
!macroend

!macro DBSAUR_SCOPE_FUNCTIONS
!include nsDialogs.nsh
Var ScopeCurrentUser
Var ScopeAllUsers
Var ScopeExplicit
Var ScopeArguments

!insertmacro DBSAUR_ELEVATE "" ""
!insertmacro DBSAUR_ELEVATE "un." "_?=$INSTDIR"

Function ScopeInit
  UserInfo::GetAccountType
  Pop $MultiUser.Privileges
  StrCpy $ScopeExplicit 0
  Call MultiUser.InstallMode.CurrentUser
  ; Preserve an existing installation for passive updates and manual upgrades.
  ReadRegStr $0 HKCU "${UNINSTKEY}" "UninstallString"
  ${If} $0 == ""
    ReadRegStr $0 HKLM "${UNINSTKEY}" "UninstallString"
    ${If} $0 != ""
      StrCpy $MultiUser.InstallMode "AllUsers"
    ${EndIf}
  ${EndIf}
  ${GetOptions} $CMDLINE "/AllUsers" $0
  ${IfNot} ${Errors}
    StrCpy $MultiUser.InstallMode "AllUsers"
    StrCpy $ScopeExplicit 1
  ${EndIf}
  ${GetOptions} $CMDLINE "/CurrentUser" $0
  ${IfNot} ${Errors}
    Call MultiUser.InstallMode.CurrentUser
    StrCpy $ScopeExplicit 1
  ${EndIf}
  ${If} $MultiUser.InstallMode == "AllUsers"
    ; Interactive setup asks before elevating. Passive/silent updates keep scope.
    ${If} $ScopeExplicit = 1
    ${OrIf} $PassiveMode = 1
    ${OrIf} ${Silent}
      Call ScopeSelectAllUsers
      ${IfThen} ${Errors} ${|} Abort ${|}
    ${EndIf}
  ${EndIf}
FunctionEnd

Function ScopeSelectAllUsers
  ${If} $MultiUser.Privileges != "Admin"
    Call ScopeElevate
    Return
  ${EndIf}
  Call MultiUser.InstallMode.AllUsers
  ClearErrors
FunctionEnd

Function ScopePage
  ${If} $PassiveMode = 1
  ${OrIf} $ScopeExplicit = 1
    Abort
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "Installation scope" "Choose who can use DBsaur on this computer."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 30u "Install for your account without administrator permission, or for everyone with administrator permission."
  Pop $0
  ${NSD_CreateRadioButton} 10u 42u 100% 14u "Only for me (no administrator permission)"
  Pop $ScopeCurrentUser
  ${NSD_CreateRadioButton} 10u 66u 100% 14u "For all users (requires administrator permission)"
  Pop $ScopeAllUsers
  ${If} $MultiUser.InstallMode == "AllUsers"
    ${NSD_Check} $ScopeAllUsers
  ${Else}
    ${NSD_Check} $ScopeCurrentUser
  ${EndIf}
  nsDialogs::Show
FunctionEnd

Function ScopePageLeave
  ${NSD_GetState} $ScopeAllUsers $0
  ${If} $0 = ${BST_CHECKED}
    Call ScopeSelectAllUsers
    ${IfThen} ${Errors} ${|} Abort ${|}
  ${Else}
    Call MultiUser.InstallMode.CurrentUser
  ${EndIf}
FunctionEnd

Function un.ScopeInit
  UserInfo::GetAccountType
  Pop $MultiUser.Privileges
  ; Match the uninstaller's directory, including when Windows copies it to TEMP.
  ReadRegStr $0 HKLM "${UNINSTKEY}" "InstallLocation"
  ${If} $0 == '$\"$INSTDIR$\"'
    ${If} $MultiUser.Privileges != "Admin"
      Call un.ScopeElevate
      ${IfThen} ${Errors} ${|} Abort ${|}
    ${EndIf}
    Call un.MultiUser.InstallMode.AllUsers
  ${Else}
    Call un.MultiUser.InstallMode.CurrentUser
  ${EndIf}
FunctionEnd
!macroend
