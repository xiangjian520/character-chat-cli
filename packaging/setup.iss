; Inno Setup script for character-chat-cli
; 用法: 先 cargo build --release, 然后 ISCC.exe packaging\setup.iss
; https://github.com/xiangjian520/character-chat-cli

#define MyAppName "Character Chat CLI"
#define MyAppVersion "0.1.1"
#define MyAppPublisher "xiangjian520"
#define MyAppURL "https://github.com/xiangjian520/character-chat-cli"
#define MyAppExeName "character-chat-cli.exe"
#define SourceRoot ".."

[Setup]
AppId={{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
LicenseFile={#SourceRoot}\LICENSE
SetupIconFile={#SourceRoot}\icon\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
OutputDir={#SourceRoot}\dist
OutputBaseFilename=CharacterChatCLI-Setup-{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
chinesesimplified.CreateDesktopIcon=创建桌面快捷方式(&D)
chinesesimplified.AdditionalIcons=附加图标
chinesesimplified.LaunchProgram=运行 {#MyAppName}
english.CreateDesktopIcon=Create a &desktop shortcut
english.AdditionalIcons=Additional icons
english.LaunchProgram=Launch {#MyAppName}

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceRoot}\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\icon\app.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\personas\*"; DestDir: "{app}\personas"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\app.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\app.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}\data"
Type: filesandordirs; Name: "{app}\plugins"
Type: files; Name: "{app}\config.json"
