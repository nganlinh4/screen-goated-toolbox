; Screen Goated Toolbox Installer
!include "MUI2.nsh"

!ifndef APP_VERSION
!error "APP_VERSION must be provided, for example /DAPP_VERSION=5.4.3"
!endif

!define TARGET_SUBDIR "x86_64-pc-windows-msvc\\release"
!define INSTALLER_NAME "screen-goated-toolbox-installer-x64.exe"

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
