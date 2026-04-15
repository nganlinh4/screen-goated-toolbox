; Screen Goated Toolbox Installer
!include "MUI2.nsh"

!ifndef APP_ARCH
!define APP_ARCH "x64"
!endif

!ifndef APP_VERSION
!define APP_VERSION "1.6"
!endif

!if "${APP_ARCH}" == "arm64"
!define VC_REDIST "vc_redist.arm64.exe"
!define TARGET_SUBDIR "aarch64-pc-windows-msvc\\release"
!define INSTALLER_NAME "screen-goated-toolbox-installer-arm64.exe"
!else
!define VC_REDIST "vc_redist.x64.exe"
!define TARGET_SUBDIR "x86_64-pc-windows-msvc\\release"
!define INSTALLER_NAME "screen-goated-toolbox-installer-x64.exe"
!endif

; Basic Settings
Name "Screen Goated Toolbox"
OutFile "target\${TARGET_SUBDIR}\${INSTALLER_NAME}"
InstallDir "$PROGRAMFILES\ScreenGoatedToolbox"
RequestExecutionLevel admin
Icon ".\assets\app.ico"

; MUI Settings
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_LANGUAGE "English"

; Installer Sections
Section "Install Application"
  SetOutPath "$INSTDIR"
  
  ; Copy main executable
  File "target\${TARGET_SUBDIR}\screen-goated-toolbox.exe"
  
  ; Copy Visual C++ Runtime and install it
  File "${VC_REDIST}"
  DetailPrint "Installing Visual C++ Runtime..."
  ExecWait "$INSTDIR\${VC_REDIST} /quiet /norestart" $0
  Delete "$INSTDIR\${VC_REDIST}"
  
  ; Create Start Menu shortcut
  CreateDirectory "$SMPROGRAMS\Screen Goated Toolbox"
  CreateShortcut "$SMPROGRAMS\Screen Goated Toolbox\Screen Goated Toolbox.lnk" "$INSTDIR\screen-goated-toolbox.exe"
  CreateShortcut "$SMPROGRAMS\Screen Goated Toolbox\Uninstall.lnk" "$INSTDIR\uninstall.exe"
  
  ; Create Desktop shortcut (optional, uncomment if desired)
  ; CreateShortcut "$DESKTOP\Screen Goated Toolbox.lnk" "$INSTDIR\screen-goated-toolbox.exe"
  
  ; Write uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"
  
  ; Write registry entry for Add/Remove Programs
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGoatedToolbox" "DisplayName" "Screen Goated Toolbox"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGoatedToolbox" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGoatedToolbox" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGoatedToolbox" "DisplayVersion" "${APP_VERSION}"
SectionEnd

; Uninstaller Section
Section "Uninstall"
  Delete "$INSTDIR\screen-goated-toolbox.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  
  Delete "$SMPROGRAMS\Screen Goated Toolbox\Screen Goated Toolbox.lnk"
  Delete "$SMPROGRAMS\Screen Goated Toolbox\Uninstall.lnk"
  RMDir "$SMPROGRAMS\Screen Goated Toolbox"
  
  Delete "$DESKTOP\Screen Goated Toolbox.lnk"
  
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGoatedToolbox"
SectionEnd
