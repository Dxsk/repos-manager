; repos-manager Windows installer (NSIS 3, MUI2, stock plugins only).
;
;   makensis -DVERSION=1.0.0 -DARCH=x86_64 -DBINARY=/abs/path/repos-manager.exe
;            -DOUTFILE=/abs/path/repos-manager-x86_64-setup.exe repos-manager.nsi
;
; Optional: -DVIVERSION=1.0.0.0 (numeric, needed for pre-release versions)
; and -DLICENSE_FILE=path. Relative paths resolve against this script's
; directory. On Windows, makensis also accepts /D instead of -D.
;
; Per-user install (no UAC prompt): it matches install.ps1, and the built-in
; self-updater can replace the exe in place without admin rights, which it
; could not do under Program Files.

!ifndef VERSION
  !error "Pass the version with -DVERSION=x.y.z"
!endif
!ifndef BINARY
  !error "Pass the exe path with -DBINARY=path/to/repos-manager.exe"
!endif
!ifndef LICENSE_FILE
  !define LICENSE_FILE "..\LICENSE"
!endif
; Windows version resources need a purely numeric a.b.c.d.
!ifndef VIVERSION
  !define VIVERSION "${VERSION}.0"
!endif
!ifndef ARCH
  !define ARCH "x86_64"
!endif
!ifndef OUTFILE
  !define OUTFILE "repos-manager-${ARCH}-setup.exe"
!endif

!define NAME "repos-manager"
!define PUBLISHER "Dxsk"
!define URL "https://github.com/Dxsk/repos-manager"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

Name "${NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${NAME}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user

VIProductVersion "${VIVERSION}"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "MIT License"

!include "MUI2.nsh"
!include "x64.nsh"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_TEXT "${NAME} has been installed and added to your PATH.$\r$\n$\r$\nOpen a new terminal and run: repos-manager --help"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; PATH is edited through PowerShell rather than ReadRegStr/WriteRegExpandStr:
; stock NSIS strings are capped at 1024 chars, so a long user PATH would be
; truncated and clobbered. The raw (unexpanded) value is kept as REG_EXPAND_SZ
; so %VAR% entries survive, and setting then clearing a dummy variable makes
; .NET broadcast WM_SETTINGCHANGE so new terminals see the change.
!define PS_PATH_HEAD "$$k=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment'); $$d='$INSTDIR'; $$a=@([string]$$k.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) -split ';' | Where-Object { $$_ -and $$_ -ne $$d })"
!define PS_PATH_TAIL "[Environment]::SetEnvironmentVariable('REPOS_MANAGER_TMP','1','User'); [Environment]::SetEnvironmentVariable('REPOS_MANAGER_TMP',$$null,'User')"
!define PS "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "${NAME} requires 64-bit Windows."
    Abort
  ${EndIf}
FunctionEnd

Section "Install"
  SetOutPath "$INSTDIR"
  File "/oname=repos-manager.exe" "${BINARY}"
  File "/oname=LICENSE.txt" "${LICENSE_FILE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  DetailPrint "Adding $INSTDIR to the user PATH"
  nsExec::ExecToLog `${PS} "${PS_PATH_HEAD}; $$k.SetValue('Path', (($$a + $$d) -join ';'), 'ExpandString'); ${PS_PATH_TAIL}"`
  Pop $0
  ${If} $0 != 0
    DetailPrint "Could not update PATH (exit $0). Add $INSTDIR to PATH manually."
  ${EndIf}

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "${URL}"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\repos-manager.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ; Rough size in KB, good enough for Add/Remove Programs.
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 10240
SectionEnd

Section "Uninstall"
  nsExec::ExecToLog `${PS} "${PS_PATH_HEAD}; $$k.SetValue('Path', ($$a -join ';'), 'ExpandString'); ${PS_PATH_TAIL}"`
  Pop $0

  Delete "$INSTDIR\repos-manager.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
