#define NAME "kalast"
#define VERSION "0.3.8-beta"
#define URL "https://github.com/GregoireHENRY/kalast"
#define PUBLISHER "NT Productions"

[Setup]
Uninstallable=no
AppId={{2F8C40BA-A831-42C0-AECA-C6265F3ABABC}
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
OutputBaseFilename=setup-{#NAME}-v{#VERSION}
Compression=lzma
SolidCompression=yes
WizardImageFile=.\assets\kalast-vbanner.bmp
WizardImageStretch=yes

[Icons]
Name: "{group}\{#NAME}"; Filename: "{app}\{#NAME}.exe"; WorkingDir: "{app}"; IconFilename: "{app}\{#NAME}.ico"

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: ".\include\*"; DestDir: "{app}"; Flags: ignoreversion
Source: ".\include\win\*"; DestDir: "{app}"; Flags: ignoreversion