; This inno script will bundle everything inside the relative folder bundle\
; Output installer file name should be given by command line /F"setup.exe"
; Assets are looked in relative folder assets\
; In particular, assets\vbanner-intro.bmp is looked for.

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
OutputBaseFilename=setup.exe
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