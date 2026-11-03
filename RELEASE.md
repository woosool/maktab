# Maktab 0.1.1 | مكتب

[English](#english) · [Français](#français) · [العربية](#العربية)

---

## English

**Maktab** is a free, open-source desktop office suite by [Woosool](https://woosool.com). It runs on macOS, Windows, Linux and ChromeOS, works offline, and treats Arabic and right-to-left text as a first-class citizen alongside English and French.

### What's included

| App | Purpose | Files |
| --- | --- | --- |
| **Qalam** قلم | Word processor | `.docx`, `.md`, `.txt`; exports PDF |
| **Jadwal** جدول | Spreadsheets | `.xlsx`, `.ods`, `.csv`; exports PDF |
| **Sard** سرد | Presentations | `.pptx`; exports PDF |
| **Saboura** سبورة | Whiteboard | `.sab` (pictures inside); sticky notes, diagrams, mind maps, voting; exports PDF, PNG, SVG, PowerPoint, Excel |
| **Waraq** ورق | PDF editor | `.pdf`: view, annotate, sign, edit pages |
| **Barid** بريد | Email | Your mail accounts, offline-friendly |

### Install

**macOS, Linux, ChromeOS** — in Terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.sh | sh
```

**Windows** — in PowerShell:

```powershell
irm https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.ps1 | iex
```

These commands install every app with no security warning. Or download a package below:

| Platform | Package |
| --- | --- |
| macOS (Apple silicon, Intel) | `Maktab-0.1.1-macos-aarch64.dmg`, `Maktab-0.1.1-macos-x86_64.dmg` |
| Windows (x86_64) | `Maktab-0.1.1-windows-x86_64-setup.exe` (choose the apps), portable `.zip` |
| Linux (x86_64, aarch64) | `maktab_0.1.1_amd64.deb`, `maktab_0.1.1_arm64.deb`, portable `.tar.gz` |
| ChromeOS | `Maktab-0.1.1-chromeos-x86_64.deb`, `Maktab-0.1.1-chromeos-aarch64.deb` (open it from Files with Linux turned on) |

The apps aren't signed with paid certificates yet. If you downloaded with a browser:
- **macOS:** open the app once, then System Settings › Privacy & Security › **Open Anyway**.
- **Windows:** on "Windows protected your PC", click **More info** › **Run anyway**.

Each app updates itself (Help › Check for Updates).

### Highlights

- **Native and fast:** written in Rust, with no browser engine.
- **Arabic and RTL:** bidirectional text, Arabic shaping, right-to-left interface.
- **One suite:** the apps share fonts, dictionaries, themes and interface languages (English, French, Arabic).
- **Offline translation:** built-in machine translation that runs on your computer.

### Known limitations

This is an early release. Some features are still in development (in Qalam: fields, header/footer editing, multiple sections, charts, equations, spell checking). Please report problems in the issue tracker.

### License

MIT. Free for personal and commercial use.

---

## Français

**Maktab** est une suite bureautique gratuite et open source de [Woosool](https://woosool.com). Elle fonctionne sous macOS, Windows, Linux et ChromeOS, sans connexion Internet, et prend en charge l'arabe et l'écriture de droite à gauche au même titre que le français et l'anglais.

### Contenu

| Application | Rôle | Fichiers |
| --- | --- | --- |
| **Qalam** | Traitement de texte | `.docx`, `.md`, `.txt` ; export PDF |
| **Jadwal** | Tableur | `.xlsx`, `.ods`, `.csv` ; export PDF |
| **Sard** | Présentations | `.pptx` ; export PDF |
| **Saboura** | Tableau blanc | `.sab` ; notes, diagrammes, cartes mentales, votes ; export PDF, PNG, SVG, PowerPoint, Excel |
| **Waraq** | Éditeur PDF | `.pdf` : lecture, annotations, signature, pages |
| **Barid** | Messagerie | Vos comptes de courriel |

### Installation

**macOS, Linux, ChromeOS** — dans le Terminal :

```sh
curl -fsSL https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.sh | sh
```

**Windows** — dans PowerShell :

```powershell
irm https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.ps1 | iex
```

Ces commandes installent toutes les applications sans avertissement de sécurité. Vous pouvez aussi télécharger un paquet ci-dessous (`.dmg` pour macOS, `-setup.exe` pour Windows, `.deb` pour Linux et ChromeOS).

Les applications ne sont pas encore signées par un certificat payant. Après un téléchargement par le navigateur :
- **macOS :** ouvrez l'application une fois, puis Réglages Système › Confidentialité et sécurité › **Ouvrir quand même**.
- **Windows :** sur « Windows a protégé votre ordinateur », cliquez sur **Informations complémentaires** › **Exécuter quand même**.

### Licence

MIT. Gratuit pour un usage personnel et commercial.

---

<div dir="rtl">

## العربية

**مكتب** مجموعة برامج مكتبية مجانية ومفتوحة المصدر من [وصول](https://woosool.com). تعمل على ماك وويندوز ولينكس وكروم أو إس دون الحاجة إلى اتصال بالإنترنت، وتدعم العربية والكتابة من اليمين إلى اليسار دعمًا كاملًا إلى جانب الفرنسية والإنجليزية.

### المحتوى

| البرنامج | الوظيفة | الملفات |
| --- | --- | --- |
| **قلم** | معالج النصوص | `.docx` و`.md` و`.txt`، والتصدير إلى PDF |
| **جدول** | الجداول الإلكترونية | `.xlsx` و`.ods` و`.csv`، والتصدير إلى PDF |
| **سرد** | العروض التقديمية | `.pptx`، والتصدير إلى PDF |
| **سبورة** | السبورة البيضاء | `.sab`؛ ملاحظات لاصقة ومخططات وخرائط ذهنية وتصويت |
| **ورق** | محرر PDF | قراءة ملفات PDF والتعليق عليها وتوقيعها وتحرير صفحاتها |
| **بريد** | البريد الإلكتروني | حساباتك البريدية |

### التثبيت

**ماك ولينكس وكروم أو إس** — في تطبيق Terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.sh | sh
```

**ويندوز** — في PowerShell:

```powershell
irm https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.ps1 | iex
```

تثبّت هذه الأوامر جميع البرامج دون أي تحذير أمني. ويمكنك أيضًا تنزيل حزمة من الأسفل.

البرامج غير موقّعة بشهادات مدفوعة بعد. إذا نزّلتها عبر المتصفح:
- **ماك:** افتح البرنامج مرة، ثم إعدادات النظام › الخصوصية والأمان › **الفتح على أي حال**.
- **ويندوز:** عند ظهور «حمى Windows جهاز الكمبيوتر»، انقر **مزيد من المعلومات** › **التشغيل على أي حال**.

### الرخصة

رخصة MIT. مجاني للاستخدام الشخصي والتجاري.

</div>
