; Screen Grounded Translator Installer
!include "MUI2.nsh"
!include "x64.nsh"

; Basic Settings
Name "Screen Grounded Translator"
OutFile "target\release\screen-grounded-translator-installer.exe"
InstallDir "$PROGRAMFILES\ScreenGroundedTranslator"
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
  File "target\release\screen-grounded-translator.exe"
  
  ; Copy Visual C++ Runtime and install it
  File "vc_redist.x64.exe"
  DetailPrint "Installing Visual C++ Runtime..."
  ExecWait "$INSTDIR\vc_redist.x64.exe /quiet /norestart" $0
  Delete "$INSTDIR\vc_redist.x64.exe"
  
  ; Create Start Menu shortcut
  CreateDirectory "$SMPROGRAMS\Screen Grounded Translator"
  CreateShortcut "$SMPROGRAMS\Screen Grounded Translator\Screen Grounded Translator.lnk" "$INSTDIR\screen-grounded-translator.exe"
  CreateShortcut "$SMPROGRAMS\Screen Grounded Translator\Uninstall.lnk" "$INSTDIR\uninstall.exe"
  
  ; Create Desktop shortcut (optional, uncomment if desired)
  ; CreateShortcut "$DESKTOP\Screen Grounded Translator.lnk" "$INSTDIR\screen_grounded_translator.exe"
  
  ; Write uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"
  
  ; Write registry entry for Add/Remove Programs
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGroundedTranslator" "DisplayName" "Screen Grounded Translator"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGroundedTranslator" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGroundedTranslator" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGroundedTranslator" "DisplayVersion" "1.6"
SectionEnd

; Uninstaller Section
Section "Uninstall"
  Delete "$INSTDIR\screen-grounded-translator.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  
  Delete "$SMPROGRAMS\Screen Grounded Translator\Screen Grounded Translator.lnk"
  Delete "$SMPROGRAMS\Screen Grounded Translator\Uninstall.lnk"
  RMDir "$SMPROGRAMS\Screen Grounded Translator"
  
  Delete "$DESKTOP\Screen Grounded Translator.lnk"
  
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenGroundedTranslator"
SectionEnd
