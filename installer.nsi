!include "MUI2.nsh"
!include "FileFunc.nsh"

SetCompressor /SOLID lzma

; General
Name "Lumien"
!system 'cmd.exe /c if not exist "releases\download" mkdir "releases\download"'
OutFile "releases\download\lumien_0.3.0_x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Lumien"
InstallDirRegKey HKCU "Software\Alexis\Lumien" "InstallLocation"
RequestExecutionLevel user
Unicode true

; Version Info
VIProductVersion "0.3.0.0"
VIAddVersionKey "ProductName" "Lumien"
VIAddVersionKey "CompanyName" "Alexis"
VIAddVersionKey "LegalCopyright" "Alexis"
VIAddVersionKey "FileDescription" "Lumien Installer"
VIAddVersionKey "FileVersion" "0.3.0.0"
VIAddVersionKey "ProductVersion" "0.3.0"

; Interface Settings
!define MUI_ABORTWARNING
!define MUI_ICON "lumien-app\src-tauri\icons\icon.ico"
!define MUI_UNICON "lumien-app\src-tauri\icons\icon.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish page with option to run
!define MUI_FINISHPAGE_RUN "$INSTDIR\lumien-app.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Lumien"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "English"

; Installer Section
Section "Lumien" SecLumien
    DetailPrint "Closing running Lumien instances..."
    nsExec::Exec 'taskkill /F /IM lumien-app.exe'

    SetOutPath "$INSTDIR"
    File "lumien-app\src-tauri\target\release\lumien-app.exe"
    File /nonfatal "target\release\lumien-cli.exe"
    File "README.md"
    File /oname=icon.ico "lumien-app\src-tauri\icons\icon.ico"

    ; Store installation folder
    WriteRegStr HKCU "Software\Alexis\Lumien" "InstallLocation" "$INSTDIR"

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Create Start Menu shortcut
    CreateDirectory "$SMPROGRAMS\Lumien"
    CreateShortcut "$SMPROGRAMS\Lumien\Lumien.lnk" "$INSTDIR\lumien-app.exe" "" "$INSTDIR\icon.ico" 0
    CreateShortcut "$SMPROGRAMS\Lumien\Uninstall.lnk" "$INSTDIR\uninstall.exe"

    ; Create Desktop shortcut
    CreateShortcut "$DESKTOP\Lumien.lnk" "$INSTDIR\lumien-app.exe" "" "$INSTDIR\icon.ico" 0

    ; Write Add/Remove Programs registry entries
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lumien" '"$INSTDIR\lumien-app.exe" --autostart'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "DisplayName" "Lumien"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "DisplayVersion" "0.3.0"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "Publisher" "Alexis"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "DisplayIcon" "$INSTDIR\icon.ico"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "HelpLink" "https://github.com/alexeikmun/lumien"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "URLInfoAbout" "https://github.com/alexeikmun/lumien"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "NoRepair" 1

    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien" "EstimatedSize" "$0"
SectionEnd

; Uninstaller Section
Section "Uninstall"
    DetailPrint "Closing running Lumien instances..."
    nsExec::Exec 'taskkill /F /IM lumien-app.exe'

    ; Remove files
    Delete "$INSTDIR\lumien-app.exe"
    Delete "$INSTDIR\lumien-cli.exe"
    Delete "$INSTDIR\README.md"
    Delete "$INSTDIR\icon.ico"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; Remove shortcuts
    Delete "$SMPROGRAMS\Lumien\Lumien.lnk"
    Delete "$SMPROGRAMS\Lumien\Uninstall.lnk"
    RMDir "$SMPROGRAMS\Lumien"
    Delete "$DESKTOP\Lumien.lnk"

    ; Remove registry entries
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumien"
    DeleteRegKey HKCU "Software\Alexis\Lumien"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lumien"
SectionEnd
