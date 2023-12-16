#define NAME "kalast"
#define URL "https://github.com/GregoireHENRY/kalast"
#define PUBLISHER "NT Productions"

[Setup]
Uninstallable=no
AppId=2F8C40BA-A831-42C0-AECA-C6265F3ABABC
AppName={#NAME}
AppVersion={#VERSION}
AppPublisher={#PUBLISHER}
AppPublisherURL={#URL}
AppSupportURL={#URL}
AppUpdatesURL={#URL}
CreateAppDir=yes
DefaultDirName=C:\Program Files\{#NAME}
DefaultGroupName={#NAME}
DisableWelcomePage=no
OutputDir=.
OutputBaseFilename={#SETUP_NAME}
Compression=lzma
SolidCompression=yes
WizardImageFile=assets\vbanner-intro.bmp
WizardImageStretch=yes

[Icons]
Name: "{group}\{#NAME}"; Filename: "{app}\{#NAME}.exe"; WorkingDir: "{app}"; IconFilename: "{app}\{#NAME}.ico"

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "bundle\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs