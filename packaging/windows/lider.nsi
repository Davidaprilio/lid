; The per-user Windows installer of LID (packaging/build.sh windows). No administrator rights: LID goes
; to %LOCALAPPDATA%\LID with a Start-menu entry and an entry in Apps & features.
;
; It installs over the 2.0 betas built with Tauri (the same folder, program name and uninstall key), and
; it takes the arguments LID's updater passes (lider_core::update::run_installer, also the Tauri updater's):
;   /P or /S   no questions (passive or silent)
;   /R         start LID when done
;   /UPDATE    an update (nothing else to do; accepted so the line parses)
;
;   makensis -DVERSION=2.0.0 -DVERSION_NUM=2.0.0.0 -DEXE=target\release\lider.exe -DROOT=. \
;            -DOUTFILE=dist\LID-2.0.0-x64-setup.exe packaging\windows\lider.nsi

Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "VERSION is not set"
!endif
!ifndef VERSION_NUM
  !define VERSION_NUM "0.0.0.0"
!endif
!ifndef ROOT
  !define ROOT "."
!endif

!define PRODUCT "LID"
!define PUBLISHER "David Aprilio"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define HOST "io.github.davidaprilio.lid"

Name "${PRODUCT}"
Caption "${PRODUCT} ${VERSION} Setup"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\${PRODUCT}"
RequestExecutionLevel user
BrandingText "Linux Internet Downloader ${VERSION}"

VIProductVersion "${VERSION_NUM}"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT} Setup"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "MIT License"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define MUI_ICON "${ROOT}\crates\lider-gui\icons\icon.ico"
!define MUI_UNICON "${ROOT}\crates\lider-gui\icons\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\lider.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start LID"

!insertmacro MUI_PAGE_LICENSE "${ROOT}\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Relaunch

Function .onInit
  StrCpy $Relaunch 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/P" $R1
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/R" $R1
  ${IfNot} ${Errors}
    StrCpy $Relaunch 1
  ${EndIf}
FunctionEnd

; LID may still be closing (an update starts the installer, then LID exits): wait until its program
; file can be replaced, at most about 10 s.
Function WaitForLid
  StrCpy $R2 0
  ${DoWhile} $R2 < 40
    ClearErrors
    FileOpen $R3 "$INSTDIR\lider.exe" a
    ${IfNot} ${Errors}
      FileClose $R3
      ${Break}
    ${EndIf}
    ${IfNot} ${FileExists} "$INSTDIR\lider.exe"
      ${Break}
    ${EndIf}
    Sleep 250
    IntOp $R2 $R2 + 1
  ${Loop}
FunctionEnd

Section "LID"
  SetOutPath "$INSTDIR"
  Call WaitForLid
  File "/oname=lider.exe" "${EXE}"
  File "/oname=LICENSE.txt" "${ROOT}\LICENSE"
  File "/oname=README.md" "${ROOT}\README.md"
  !ifdef THIRD_PARTY
    SetOutPath "$INSTDIR\third-party"
    File /r "${THIRD_PARTY}\*.*"
    SetOutPath "$INSTDIR"
  !endif
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\${PRODUCT}.lnk" "$INSTDIR\lider.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\lider.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/Davidaprilio/lid"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" "$0"
SectionEnd

Function .onInstSuccess
  ${If} $Relaunch == 1
    Exec '"$INSTDIR\lider.exe"'
  ${EndIf}
FunctionEnd

; The downloads list and settings (%APPDATA%\lider, %LOCALAPPDATA%\lider) stay; what LID wrote for the
; desktop goes: the autostart value and the browser helper's registration.
Section "Uninstall"
  Delete "$INSTDIR\lider.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\README.md"
  RMDir /r "$INSTDIR\third-party"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${PRODUCT}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegValue HKCU "${RUN_KEY}" "${PRODUCT}"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\${HOST}"
  RMDir /r "$LOCALAPPDATA\lider\native-host"
SectionEnd
