# Ezoo Download Manager (EDM) 1.1.2 — release notes

> DRAFT — prepared in P8 (regression + docs), local only. To be
> PREPENDED to the v1.1.2 release body in P10/P11 (the release-body
> edit is P10/P11 work, not P8). Format follows the 1.1.1 release
> notes (Arabic emoji bullets, then an English paragraph, then the
> §3.7 thanks section). Version bumped to 1.1.2 in P8-quater
> (Cargo, manifests, metainfo, site). This is a NEW version, so
> existing 1.1.1 installs WILL be offered it automatically — but
> ONLY after `version.json` is published (P8-quinquies split, see
> below): the release publishes its files first, and the updater
> manifest is published later, only after the 1.1.2 extension is
> approved in BOTH stores (Chrome Web Store + AMO). Until then the
> updater stays silent and 1.1.2 is a manual install from the
> release page. ⚠️ Keep the extension warning (first bullet, AR +
> EN) in the release body: the 1.1.1 extension stops working after
> the app update until the extension is updated to 1.1.2.

---

## ✨ ما الجديد في 1.1.2 / What's new in 1.1.2

⚠️ مهم — بعد تحديث البرنامج إلى 1.1.2، تتوقف إضافة المتصفح 1.1.1 عن العمل: يجب تحديث الإضافة إلى 1.1.2 أيضًا. ستُرفع الإضافة الجديدة إلى متجر كروم وAMO في اليوم نفسه، لكن مراجعة المتجرين قد تستغرق أيامًا إلى أسابيع؛ ويمكن تثبيت الإضافة الجديدة يدويًا من صفحة الإصدار في هذه الأثناء.
📦 النشر على مرحلتين: ملفات الإصدار تُنشر أولًا، ولا يعرض المُحدِّث داخل البرنامج 1.1.2 إلا بعد نشر `version.json` — وذلك فقط بعد اعتماد الإضافة في متجري كروم وAMO معًا. حتى ذلك الحين ثبّت 1.1.2 يدويًا من صفحة الإصدار.
🧩 تثبيت الإضافة يدويًا من الإصدار: ملف كروم `.crx` موقَّع بالمفتاح الأصلي، وملف `.zip` للتحميل غير المعبّأ (وضع المطوّر ← Load unpacked ← اختر المجلد بعد فك الضغط). أما ملف فايرفوكس `.xpi` فهو **غير موقَّع**: لا يثبَّت في فايرفوكس العادي (موزيلا تشترط التوقيع) — جرّبه مؤقتًا من `about:debugging`، أو ثبّته دائمًا على Developer Edition / Nightly بعد تعطيل `xpinstall.signatures.required`. النسخة الموقّعة من AMO تأتي بعد اعتماد المتجر.
🌐 روابط كانت تُرفض من بعض المواقع أصبحت تُنزَّل بنجاح أكثر، مع تحسينات للملفات الكبيرة وتقدير أدقّ للوقت المتبقي.
🔒 قناة المتصفح أصبحت موقَّعة (HMAC): كل طلب بين الإضافة والبرنامج موقَّع ومؤقَّت، فلا يمكن إعادة تشغيل طلب ملتقط. مراقبة الحافظة أصبحت متوقفة افتراضيًّا.
🔍 الإضافة تتحقق من الرابط قبل التحميل: تعرف فورًا إن كان الملف موجودًا وحجمه وهل يستأنف، وإن رفض الموقع التحميل (403) يفتحه في المتصفح بدل الفشل الصامت.
✂️ قصّ جزء من فيديو: من قائمة الفيديو المكتمل أو من الإضافة — قصّ سريع أو قصّ دقيق مطابق للإطار (إعادة ترميز). في الإضافة نافذتان: «كل الجودات» و«تحميل جزء» مع حقول الوقت والجودة المختارة. البث المباشر (.m3u8 / يوتيوب) لا يُقصّ بعد — النافذة تقول ذلك صراحة بدل تحميل الفيديو كاملًا.
🧲 روابط magnet: تعرض قائمة الملفات قبل التحميل لتختار ما تريد، مع تحسينات للتورنت وإصلاح تسرّب التتبّع للتورنتات الخاصة (لم تعد تُعلن للمتتبّعات الإضافية).
⚡ محرك تكيّفي أذكى: عند حدّ الخادم (429) يخفّف الاتصالات ويتذكّر الحدّ لكل مضيف، مع حدود لكل موقع وزرّ تحديث الرابط المنتهي.
🎨 هوية جديدة: الاسم أصبح «Ezoo Download Manager (EDM)» بأيقونات مسطّحة وشارات موحّدة، وإصلاح تسميات شريط الأدوات العربية (أصبحت تظهر كاملة بسطرين).
🔐 كلمات مرور المواقع والكوكيز على لينكس أصبحت محفوظة في مخزن مفاتيح النظام (Secret Service) بدل النص العادي.
🛠️ إصلاحات: عدّ مزدوج في HLS، حوارات لينكس الأصلية، حارس حفظ قائمة التحميل، وزرّ «إغلاق» في نافذة «حول» بالعربية.

Links refused by some sites now download more often, with big-file improvements and a steadier time-left estimate. The browser channel is now HMAC-signed: every request between the extension and the app is signed and time-bound, so a captured request cannot be replayed. Clipboard monitoring is off by default. The extension now checks a link before downloading: it knows at once whether the file exists, its size, and whether it resumes — and if a site refuses the download (403), it opens the link in the browser instead of failing silently. Cut a part of a video: from a completed video's menu or from the extension — a quick cut or a frame-accurate exact cut (re-encode). In the extension there are two windows: "All qualities" and "Download part" with time fields and the chosen quality. Live streams (.m3u8 / YouTube) cannot be cut yet — the window says so plainly instead of downloading the whole video. Magnet links now show the file list before downloading so you pick what you want, with torrent improvements and a fix for the private-tracker leak (private torrents no longer announce to extra trackers). A smarter adaptive engine: on a server rate limit (429) it lowers the connection count and remembers the limit per host, with per-site limits and a refresh-link button for expired links. New identity: the name is now "Ezoo Download Manager (EDM)" with flat icons and unified badges, and the Arabic toolbar labels are fixed (they now show in full on two lines). On Linux, site passwords and session cookies are now kept in the system keyring (Secret Service) instead of plain text. Fixes: an HLS double-count, native Linux dialogs, a downloads-list save guard, and the Arabic About dialog's Close button.

- ⚠️ Important — after updating the app to 1.1.2, the 1.1.1 browser extension stops working: update the extension to 1.1.2 as well. The new extension is uploaded to the Chrome Web Store and AMO the same day, but store review can take days to weeks; install the new extension manually from the release page meanwhile.
- 📦 Two-stage publish: the release files go out first, and the in-app updater offers 1.1.2 only after `version.json` is published — which happens only after the extension is approved in the Chrome Web Store and AMO together. Until then, install 1.1.2 manually from the release page.
- 🧩 Manual extension install from the release: the Chrome `.crx` is signed with the original key, and the `.zip` is for unpacked loading (Developer mode → Load unpacked → pick the unzipped folder). The Firefox `.xpi` is **unsigned**: it does not install in standard Firefox (Mozilla requires signatures) — try it temporarily from `about:debugging`, or install it permanently on Developer Edition / Nightly after disabling `xpinstall.signatures.required`. The signed AMO version comes after store approval.

## 🙏 شكر / Thanks

«Ezoo Download Manager (EDM)» فرع مستقل من برنامج «Ezoo Download» للمطوّر أحمد عزت (Ezoo Tech)، يصونه ويطوّره kerlif. شكر خاص لأحمد عزت على البرنامج الأصلي، ولكيرليف على التحسينات الإضافية.

Ezoo Download Manager (EDM) is an independent fork of Ezoo Download by Ahmed Ezzat (Ezoo Tech), maintained and extended by kerlif. Special thanks to Ahmed Ezzat for the original program, and to kerlif for the additional improvements.

## ⚠️ ملاحظات معروفة / Known notes (P8-ter — decisions recorded)

- قصّ/دمج مصادر MPEG-TS: الـ ffmpeg المضمّن (johnvansickle 7.0.2-static) قد يتعطّل عند قراءتها. منذ هذه النسخة يعيد البرنامج المحاولة تلقائيًا بـ ffmpeg النظام إن كان مثبّتًا؛ وإن لم يكن مثبّتًا تظهر رسالة واضحة تطلب تثبيته من مدير الحزم. القرار (2026-11-08): نبقي على النسخة المضمّنة الحالية؛ إذا وصلت شكوى حقيقية عن قصّ/دمج TS أو HLS ننتقل إلى BtbN 8.1.3 — انظر `docs/OPEN-QUESTIONS.md` (Q71).
- Cutting/merging MPEG-TS sources: the bundled ffmpeg (johnvansickle 7.0.2-static) may crash while reading them. From this version the app automatically retries with the system ffmpeg when installed; when none is installed it shows a clear message asking to install ffmpeg via the package manager. Decision (2026-11-08): keep the current bundled build; if a real complaint about TS/HLS merge/cut arrives, switch to BtbN 8.1.3 — see `docs/OPEN-QUESTIONS.md` (Q71).
- 📦 AppImage ومخزن مفاتيح النظام (libsecret): اعتماد `.deb` و`.rpm` على `libsecret-1-0` مصرَّح به، أما AppImage فلم يُتحقَّق بعد من أنه يضمّ المكتبة ويعمل مع مخزن المفاتيح — يبقى «غير متحقق منه» حتى يُختبَر على آلة حقيقية.
- 📦 AppImage and the system keyring (libsecret): the `.deb` and `.rpm` declare the `libsecret-1-0` dependency, but the AppImage has not been verified to bundle the library and work with the keyring — it stays UNVERIFIED until tested on a real machine.
- 📦 حزمة Flatpak أُزيلت من 1.1.2 بقرار المشرف (2026-11-08) — لن تُبنى ولن تُشحن؛ حزمة Flatpak الخاصة بـ 1.1.1 على GitHub تبقى كما هي.
- The Flatpak package was removed from 1.1.2 by the maintainer's decision (2026-11-08) — it will not be built or shipped; the released v1.1.1 Flatpak asset on GitHub is untouched.
- 📦 النشر على مرحلتين (قرار 2026-11-08، P8-quinquies): إصدار الوسم ينشر كل الملفات **ما عدا** `version.json`؛ ملف `version.json` الموقَّع يُنشر لاحقًا عبر مهمة `publish-updater-manifest` (تشغيل يدوي من main) فقط بعد اعتماد الإضافة في متجري كروم وAMO معًا، ويُبنى من ملف التثبيت المنشور نفسه (تنزيله وحساب بصمته وتوقيعه) لا من بناء جديد. حتى ذلك الحين لا يعرض المُحدِّث أي تحديث (غياب الملف = فشل صامت، لا خطأ ولا عرض معطوب) — انظر `docs/UPDATE-CHECKER.md` (§14).
- Two-stage publish (decision 2026-11-08, P8-quinquies): the tag release publishes every file **except** `version.json`; the signed `version.json` is published later by the `publish-updater-manifest` dispatch job (manual run from main) only after the extension is approved in the Chrome Web Store and AMO together, and it is built from the published installer itself (downloaded, hashed, signed), never from a rebuild. Until then the updater offers no update (a missing manifest is a silent fail, not an error and not a broken offer) — see `docs/UPDATE-CHECKER.md` (§14).
