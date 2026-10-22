<div align="center">

<img src="assets/logo-wordmark.svg" alt="ZDM Logo" width="700"/>

# Zyzoom Download Manager (ZDM)
### مدير تحميل فائق السرعة — بديل عصري ومتفوق على Internet Download Manager
**Developed by Ramy Badraan**

[![License](https://img.shields.io/badge/License-Proprietary-blue.svg)](#)
[![Stack](https://img.shields.io/badge/Engine-Tauri%202%20%7C%20Rust%20%7C%20Tokio-orange.svg)](#)
[![UI](https://img.shields.io/badge/Frontend-React%2018%20%7C%20TypeScript%20%7C%20Tailwind-cyan.svg)](#)
[![Platform](https://img.shields.io/badge/Platform-Arch%20Linux%20%7C%20Fedora%20%7C%20Ubuntu%20%7C%20Windows-green.svg)](#)

</div>

---

## ⚡ لماذا يتفوق ZDM على Internet Download Manager (IDM)؟

| الميزة | Internet Download Manager (IDM) | Zyzoom Download Manager (ZDM) |
|---|---|---|
| **سرعة الانتهاء** | يعاني من تعليق وتوقف طويل عند 99% بسبب دمج الأجزاء المؤقتة | **Zero-Lag Write**: يكتب مباشرة في موضع البايت المحدد دون أي وقت دمج! |
| **تقنية التوازي** | ثابتة وغالباً ما يتعطل آخر جزء بمفرده | **Work Stealing**: إعادة تقسيم الجزء الأبطأ بين القنوات الحرة تلقائياً |
| **القنوات المتزامنة** | أقصى حد عملي محدود | **حتى 32 قناة متزامنة (Turbo Mode)** مع توفير استهلاك الشبكة |
| **استهلاك الذاكرة (RAM)** | قد يستهلك مئات الميغابايت مع الواجهات القديمة | **خفيف جداً (< 90MB RAM)** بفضل أمان وخفة لغة Rust |
| **دعم الأنظمة** | ويندوز فقط واحتكاري | **Cross-Platform**: يعمل بأداء أصلي كامل على Arch Linux, Fedora, Ubuntu, و Windows |
| **التشغيل وشريط المهام** | Tray قديم | **شريط مهام ذكي (Waybar / Hyprland / KDE / GNOME / Windows Taskbar)** |
| **تثبيت الإضافة** | يتطلب خطوات يدوية معقدة أحياناً | **تثبيت تلقائي بالكامل (Auto-Install)** لجميع المتصفحات فور التنصيب |
| **تحميل الفيديو والوسائط** | التقاط محدود قد يواجه مشاكل مع التحديثات | **محرك مدمج قوي** عبر yt-dlp لاستخراج أعلى الجودات (4K/8K/Audio) |

---

## 📦 التثبيت على Arch Linux وكافة توزيعات لينكس

تم إعداد سكربت تنصيب شامل وذكي [`install.sh`](file:///home/ramy/zdm/install.sh) يتعرف تلقائياً على توزيعتك (Arch, Fedora, Debian/Ubuntu, openSUSE):

### التثبيت السريع:
```bash
./install.sh
```

أو لتثبيت الحزم والمكتبات المفقودة تلقائياً عبر مدير الحزم:
```bash
./install.sh --deps
```

### ما يقوم به سكربت التنصيب:
1. **التعرف على التوزيعة** وتثبيت متطلبات النظام (`webkit2gtk-4.1`, `libayatana-appindicator`).
2. **بناء وتثبيت الثنائيات** فائقة السرعة `zdm` و `zdm-host`.
3. **تثبيت الأيقونات وقائمة التطبيقات** في معيار XDG الرسمي (`/usr/share/applications/zdm.desktop`).
4. **تفعيل التشغيل التلقائي مع إقلاع النظام**:
   - يتم إنشاء ملف تشغيل تلقائي في `~/.config/autostart/zdm.desktop` بأمر `zdm --minimized`.
   - يشتغل البرنامج في الخلفية فور تشغيل النظام ويظهر مباشرة في **أيقونة الـ Waybar** أو **شريط مهام ويندوز / لينكس**.
5. **تثبيت إضافة المتصفح تلقائياً**:
   - تسجيل المضيف `NativeMessagingHosts` و `External Extensions` في جميع المتصفحات الموجودة على الجهاز (Google Chrome, Chromium, Brave, Microsoft Edge, Vivaldi, Firefox).
   - لا حاجة لأي إعداد يدوي! بمجرد فتح المتصفح تكون الإضافة مفعلة ومتصلة بـ ZDM فوراً.

### لإلغاء التثبيت:
```bash
./install.sh --uninstall
```

---

## 🖥️ تشغيل أيقونة ZDM في Waybar (Hyprland / Sway)

يدعم ZDM معيار **StatusNotifierItem (SNI)** المعتمد عالمياً في Wayland ولينكس:
1. في ملف إعداد Waybar لديك (`~/.config/waybar/config`):
   تأكد فقط من وجود وحدة `"tray"` ضمن شريطك، مثلاً:
   ```json
   "modules-right": [
       "pulseaudio",
       "network",
       "tray",
       "clock"
   ],
   "tray": {
       "icon-size": 18,
       "spacing": 10
   }
   ```
2. ستظهر أيقونة ZDM النيون الصغيرة فور تشغيل البرنامج.
3. الضغط على الأيقونة يُظهر أو يُخفي الواجهة، والزر الأيمن يفتح قائمة التحكم السريعة (إضافة رابط، إظهار/إخفاء، والخروج).
4. عند الضغط على زر الإغلاق `[X]` في نافذة ZDM، يتم تصغيرها للـ Tray في الخلفية دون قطع التحميلات (مثل سلوك IDM تماماً).

---

## 🪟 نسخة ويندوز — ملف تنصيب كامل (NSIS Setup Installer)

نسخة ويندوز ليست نسخة محمولة (Portable)، بل تأتي كملف تنصيب تنفيذي كامل مدمج به معالج التثبيت (NSIS Setup):
- **معالج تنصيب رسمي كامل**: `ZDM-1.0.0-Setup.exe` يدعم اللغتين العربية والإنجليزية.
- **التثبيت في ملفات البرامج**: `C:\Program Files\Zyzoom Download Manager`.
- **اختصارات سطح المكتب وقائمة ابدأ**، مع التسجيل التلقائي في قائمة البرامج المثبتة في ويندوز (Add or Remove Programs) مع أداة `uninstall.exe`.
- **أيقونة شريط المهام (Windows Notification Tray)** مع دعم التشغيل مع بدء تشغيل ويندوز (Startup).
- لبناء حزمة التثبيت الخاصة بويندوز:
  ```bash
  npm run build
  ```

---

## ⚖️ حقوق الملكية والتطوير

تم تطوير وتصميم هذا البرنامج بالكامل بواسطة:
**Developed by Ramy Badraan**
جميع الحقوق محفوظة للمطور رامي بدران © 2026.
