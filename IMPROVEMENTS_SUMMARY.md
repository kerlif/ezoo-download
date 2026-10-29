# ملخّص التحسينات — Ezoo Download 1.1.0

> النسخة العربية أولًا، ثم النسخة الإنجليزية أدناه.
> Arabic first, English below.
>
> الفرع: `v110-upgrade` في المستودع الفرعي `kerlif/ezoo-download`.
> الترخيص: MIT © 2026 Ahmed Ezzat (Ezoo Tech). كل حقوق النشر
> والإشادات محفوظة للمؤلف الأصلي والمستودع الأصلي
> `AhmedEzo686/ezoo-download`؛ لم تتغيّر أي شروط ترخيص.
> هذا الملف يلخّص ما فعلته مراحل الترقية 0–10 فعلًا، وفق تقارير
> المراحل في `docs/`؛ لا ادّعاءات جديدة هنا.

## 1. تنظيف آمن (المرحلة 1)

- تنسيق كامل للشجرة بـ `cargo fmt`، وإصلاح كل تحذيرات `clippy`
  (بما فيها مسارات ويندوز فقط عبر `#[cfg(windows)]`)، وإعادة تسمية
  خصائص Slint المهملة.
- رقم الإصدار 1.1.0 في الشيفرة والتغليف. لا تغيير في السلوك.
- الاختبارات: 83 ناجحة / 0 فاشلة (كما في خط الأساس).

## 2. إزالة الواجهات الترويجية والتبرّع (المرحلة 2)

- أُزيلت كتلة التبرّع من نافذتَي About وWhat's New، وأزرار قناتَي
  YouTube وFacebook من نافذة About، مع معالجات `open-link`
  المرتبطة بها، و6 مفاتيح ترجمة، وصورة رمز InstaPay.
- بقيت كل الإشادات: «Coded by Ahmed Ezzat»، وروابط المستودع
  الأصلي، وبيانات الناشر في المثبّت (ليست واجهة ترويجية ظاهرة
  للمستخدم).
- أُعيد توليد ملفات الترجمة `.po` (473 ← 468 نصًا، 0 مفقود).

## 3. الخطوط المضمّنة (المرحلة 3)

- أُزيل خط InstaArabi (لا ترخيص منشورًا له: جدول الاسم يقرأ
  «All Rights Reserved» ولا يوجد نص ترخيص) وملفّا «Times New Roman»
  المُعاد تسميتهما (وهما في الحقيقة Liberation Serif باسم عائلة
  مُنتحَل).
- الخطوط الجديدة، كلها SIL OFL 1.1 وتُشحَن كما هي دون تعديل:
  Carlito (400/700) للنص اللاتيني، وNoto Sans Arabic (400/700،
  إصدار v2.013) للنص العربي، وLiberation Serif (400/700) كنص
  مستندات اختياري فقط.
- التسجيل الآن في الذاكرة عند بدء التشغيل عبر مجموعة fontique
  المشتركة في Slint، مع بدائل حسب النص (العربي ← Noto Sans
  Arabic، واللاتيني ← Carlito)؛ لم يعد البرنامج يكتب خطوطًا في
  مجلد خطوط المستخدم، ويحذف النسخ القديمة المثبّتة، وتهاجر
  الإعدادات المحفوظة القديمة تلقائيًا.
- أُثبت بالاختبار أن الواجهة تُرسَم بالكامل من الخطوط المضمّنة
  (لقطات متطابقة البكسل مع صفر خطوط نظام قابلة للاستخدام).
  استثناء موثّق: اختبار «صفر خطوط» الحرفي يفشل داخل مكتبة
  fontique نفسها قبل أي شيفرة تطبيق (انظر «قيود معروفة»).

## 4. حجم الملف وبيانات البث في نافذة الالتقاط (المرحلة 4)

- فحص HEAD أولًا (طول المحتوى)، ثم تأكيد عبر طلب GET جزئي
  (`Content-Range`)؛ طول HEAD كاذب يُصحَّح، وطول غير مؤكَّد
  يُعلَم «غير موثّق» ويُعرَض كتقدير.
- وحدة `stream_meta` جديدة: تحليل قوائم HLS الرئيسية والفرعية
  (يُفضَّل `AVERAGE-BANDWIDTH`، وتُضاف بتّات مجموعة الصوت المنفصلة
  بعد أخذ عينات من مقاطعها)، وتحليل بيانات DASH متعددة الفترات
  (مجموع المدد، والفيديو والصوت منفصلان)، والحجم = (بتّات
  الفيديو + الصوت) × المدة / 8 ويُسمّى دائمًا تقديرًا مع مصدره.
  ميزانية الفحص ~5 ثوانٍ خارج خيط الواجهة.
- نافذة الإضافة تعرض الآن: المدة، ومحدِّد جودة بكل المتغيرات،
  والحجم المقدَّر للجودة المختارة. **محدِّد الجودة للعرض فقط**:
  الاختيار لا يوجّه التنزيل بعد (انظر «قيود معروفة»).
- رفض DRM: مخططات Widevine/PlayReady/FairPlay وصيغ مفاتيح HLS
  المحمية تُعطّل الأزرار وترفض التنزيل مبكرًا؛ AES-128 ما زال
  مدعومًا. لا التفاف على DRM.
- تقوية أسماء الملفات (`..` تُزال، و`safe_join` موثّق داخل مجلد
  التنزيل)، وحجم yt-dlp المعروض في لوحة المتصفح.
- الاختبارات: 105 ناجحة / 0 فاشلة (19 جديدة بملقّم محلي).

## 5. محرك التجزئة التكيفية — غرفة نظيفة (المرحلة 5)

صُمّم من المبادئ الأولى وبكلمات المؤلف الخاصة (انظر
`docs/ENGINE.md`)؛ لم تُنسَخ أو تُدرَس أي شيفرة خارجية، ولا يُذكَر
اسم أي منتج آخر في الواجهة أو الموقع.

- **مقياس الاتصالات**: يبدأ بـ 2–4 اتصالات، ثم يجرّب إضافة
  اتصال ويقيس النافذة التالية (3 ثوانٍ): ارتفاع ≥10% ← جرّب
  مجددًا؛ انخفاض ← أنقص واحدًا؛ ثبات لنافذتين ← توقّف عن النمو.
  السقف 16 لكل مضيف و32 مطلقًا. الإزالة سلسة (لا قَتل
  لاتصال منتصف مقطع).
- **التراجع**: 429/503 مع تحليل `Retry-After` (ثوانٍ أو تاريخ
  HTTP، بحد أقصى 300 ثانية) + اهتزاز حتى 25%؛ إعادة ضبط متتالية
  (3) تخفّض العدد. لا استهلاك لمحاولات إعادة على تقييد المعدل.
- **التقسيم الديناميكي**: العامل العاطل يقطع أكبر مقطع نشط عند
  منتصف ما تبقّى (بحد أدنى 1 MiB) تحت قفل واحد، دون فجوات أو
  تداخل، وجدول المقاطع يُحفَظ قبل الكتابة.
- **إنقاذ التوقّف**: مراقب لكل مقطع (أساس سرعة من أول بايتاته،
  ومرجع = الأعلى بين أساسه ومتوسط أقرانه)؛ مقطع أبطأ من 15% من
  المرجع لـ 10 ثوانٍ متتالية (بعد مهلة 10 ثوانٍ) يُعلَّم
  للاستيلاء ويُعاد طلبه من آخر بايت مكتوب — دون فقدان بيانات.
- **التعلّم لكل مضيف**: محلي فقط في `settings.json`
  (`host_learning`)، متوسّطات أسّية بوزن 0.3، وانتهاء بعد 30
  يومًا، والمفتاح اسم المضيف فقط؛ لا شبكة إطلاقًا. قيود الخادم
  الصريحة التي يضبطها المستخدم تبقى سقوفًا.
- **التحقق من الاستئناف**: `validate_segment_map` يرفض أي جدول
  مقاطع فيه فجوة أو تداخل أو خروج عن الحدود ويعيد التخطيط من
  الصفر. بلا دعم Range أو بحجم مجهول: اتصال واحد فقط كما قبل.
- **المعايير** (ملقّم محلي، 3 جولات لكل إعداد، والتحقق بـ SHA-256
  في كل جولة — انظر `docs/BENCHMARKS.md`): على ملف 8 MiB
  بمقدار محدود لكل اتصال، التكيفي 4.315 ث/متوسط (ذروة 10
  اتصالات) مقابل ثابت-1: 23.367، وثابت-4: 5.824، وثابت-8:
  2.904، وثابت-16: 1.466. وعلى ملف 32 MiB سريع، التكيفي 1.797
  ث (ذروة 7) مقابل ثابت-1: 8.589، وثابت-4: 2.207، وثابت-8:
  1.110، وثابت-16: 0.556. التكيفي يتفوّق على العدد الثابت
  المنخفض، ويخسر أمام 8/16 المختارَين جيدًا — هذا مُبلَّغ
  بصراحة في التقرير. التعلّم عبر الجولات لم يُقَس.
- لا اعتماديات جديدة (sha2 اعتماد تطوير فقط). الاختبارات: 133
  ناجحة / 0 فاشلة.

### معاملات المحرك القابلة للضبط (`AdaptiveConfig`)

| المعامل | الافتراضي |
|---|---|
| `initial_connections` / `min_initial_connections` | 4 / 2 |
| `measure_window` / `gain_threshold` / `no_gain_intervals` | 3 ث / 1.10 / 2 |
| `per_host_max` / `absolute_ceiling` | 16 / 32 |
| `dynamic_split_min_remaining` | 1 MiB |
| `stall_fraction` / `stall_seconds` / `stall_grace` | 0.15 / 10 ث / 10 ث |
| `rate_limit_default_backoff` / `max_retry_after` / `jitter_fraction` | 5 ث / 300 ث / 0.25 |
| `learning_alpha` / `learning_expiry_days` / `reset_reduction_after` | 0.3 / 30 يومًا / 3 |

## 6. الواجهة والوضع الداكن وإصلاح نوافذ Wayland (المرحلة 6)

- **إصلاح Wayland**: كل نافذة حوار تُنشَأ الآن عند أول فتح
  فقط (`LazyWeak` + `ensure_*`) بدل ~20 نافذة عند بدء التشغيل.
  السبب الجذري موثّق: خلفية Slint/winit تُنشئ كل النوافذ غير
  النشطة، وWayland يتجاهل «غير مرئي» فيُعيَّن ~20 مستوى أعلى
  (toplevel) دفعة واحدة كألسنة مسطّحة جنبًا إلى جنب. أُثبت
  الإصلاح تحت Xvfb (نافذة واحدة عند البدء، ونافذة واحدة لكل
  حوار يُفتَح، وإعادة الاستخدام عند إعادة الفتح)؛ التحقق على
  KDE Plasma (Wayland) الحقيقي ما زال معلّقًا.
- شريط مقاطع في نافذة التفاصيل: امتدادات ملوّنة لكل اتصال
  (جارٍ التنزيل / مكتمل / متوقف / في الانتظار) مع مفتاح، للقراءة
  فقط ودون تغيير في سلوك المحرك.
- إصلاح فيض شريط الأدوات (927px ≤ 960px الافتراضية)، وتدقيق
  الألوان: كل أزواج النص/الخلفية في السمات التسع ≥4.5:1 (انظر
  `docs/THEMES.md`)؛ قيم الوضع الداكن لم تتغيّر.
- الاختبارات: 134 ناجحة / 0 فاشلة.

## 7. فاحص التحديثات (المرحلة 7) — نموذج الأمان

- المصدر الوحيد: إصدارات المستودع الفرعي
  (`kerlif/ezoo-download`) من ثابت `REPO` واحد؛ لا فاحص ثانيًا.
- التسلسل: إعلان الإصدار عبر API ← بيان `version.json` الموقَّع
  ← ملف `SHA256SUMS`، وكلها عبر `User-Agent: EzooDownload/<version>`
  فقط، مع ذاكرة ETag (`checker-cache.json`)، وإعادة توجيه يدوية
  (3 قفزات كحد أقصى، لنفس المصدر أو مضيفي أصول GitHub فقط).
- التحقق قبل أي عرض: توقيع Ed25519 (ring) على البيان يغطي
  الإصدار وبصمة مثبّت ويندوز؛ ثم مطابقة SHA-256 المزدوجة
  (البيان الموقَّع + ملف المجاميع) للملف المنزَّل، مع إعادة
  فحص قبل التثبيت. بيان بلا توقيع لا يُعرَض أبدًا.
- الجدولة: أول فحص بعد 3 ثوانٍ، ثم كل 24 ساعة، مع سلّم تراجع
  (5د/15د/1س/6س/24س) و`Retry-After` يحسم (محصور 1د–24س)؛ كل
  الفشل صامت. قابل للإيقاف من الخيارات > عام (مفعّل افتراضيًا).
- سلوك المنصات: ويندوز المثبَّت يسأل قبل التثبيت وينتظر خمول
  التنزيلات؛ AppImage يُستبدَل ذرّيًا بعد التحقق والسؤال؛
  .deb/.rpm/Flatpak/غير المعروف: إشعار فقط مع رابط صفحة
  الإصدار. النسخ المحمولة: إشعار فقط.
- زوج مفاتيح Ed25519 جديد خاص بالفرع (المفتاح العام ملتزَم به
  في `selfupdate.rs`، والخاص خارج الشجرة في ملف بصلاحية 600،
  والسر `UPDATE_SIGNING_KEY` ما زال بانتظار الإنشاء عبر
  المتصفح). حتى يوجد، ملفات `version.json` بلا توقيع ويرفضها
  المحدِّث داخل التطبيق (التثبيت اليدوي يعمل).
- الاختبارات: 156 ناجحة / 0 فاشلة (22 جديدة، منها 13 بملقّم
  محلي تغطي عدم الاتصال/الحدّ/التوقيع السيئ/إعادة التوجيه
  العابرة للمضيف/ETag).

## 8. Flatpak وCI (المرحلة 8 — مُعَدّ، بانتظار CI)

- هوية `io.github.kerlif.EzooDownload`، وزمن تشغيل Freedesktop
  26.08، وبناء دون اتصال بمصادر مُورَّدة (709/709 حزمة، وكل
  البصمات تطابق `Cargo.lock`).
- الأذونات: `dri` و`ipc` و`network` وWayland/X11 الاحتياطي
  و`xdg-download` فقط (لا `home` ولا `host`)، مع بوابة اختيار
  الملفات واسم StatusNotifier للدرج.
- لا `ffmpeg-full` (متقاعد لـ 25.08+)؛ ترميزات `codecs-extra`
  تُثبَّت تلقائيًا مع زمن التشغيل — ووجود ثنائي `ffmpeg` على
  `PATH` داخل الصندوق غير موثّق (سؤال مفتوح 13).
- yt-dlp: لا نسخة مثبّتة داخل الحزمة؛ الجلب التلقائي الموجود
  المفحوص بالبصمة إلى مجلد بيانات الصندوق هو المورِّد (قرار
  السؤال 1).
- سير عمل CI مُصعَّدة عبر المتصفح (لا يستطيع الموصل دفع
  `.github/workflows/*`): بناء Flatpak أولًا في مهمة لينكس،
  وحزمة `.flatpak` تصل إلى الإصدار عبر `dist/*` و`SHA256SUMS.txt`،
  وفحص clippy واختبارات كاملة على ويندوز، وخطأ صارم عند غياب
  مفتاح التوقيع في الإصدارات. الوسم `v1.1.0` لم يُنشَأ بعد،
  ولم تعمل أي مهمة CI — المرحلة 8 ليست «ناجحة» حتى تقرير
  تحقق CI.

## 9. إعادة تصميم الموقع (المرحلة 9)

- إعادة بناء بلا إطار (HTML/CSS/JS خام، صفر خطوة بناء، لا
  متتبّعات، لا نصوص طرف ثالث، لا خطوط خارجية) في `docs/`.
- ثنائي اللغة (العربية RTL افتراضيًا، والإنجليزية LTR) بقاموس
  176 مفتاحًا يبدّل كل النصوص و`lang`/`dir` والعنوان، ويُحفَظ
  في `localStorage`؛ وسمتان فاتح/داكن (تباين ≥4.5:1 لكل الأزواج
  المحسوبة)، ورابط تخطٍّ، ومعالم دلالية، وتركيز مرئي، وتشغيل
  بلوحة المفاتيح.
- الأقسام: البطل، المزايا، كيف يعمل (مع المحاكي التفاعلي
  المحفوظ: منزلق، واختيار حجم، ومسارات، ورسم بياني، ومقارنة
  اتصال واحد)، والتنزيلات مع كشف نظام التشغيل، وأدلة التثبيت
  (ويندوز/.deb/.rpm/AppImage/Flatpak/الإضافة) بأوامر قابلة
  للنسخ وروابط بأسماء ملفات الإصدار 1.1.0 الثابتة، والأسئلة
  الشائعة (بإجابات موثّقة فقط، منها محدِّد الجودة للعرض فقط
  ورفض DRM)، وملخص الخصوصية، ورابط سجل التغييرات.
- خطوط WOFF2 الأربعة مولَّدة محليًا من أصول المرحلة 3 (عدد
  الحروف متطابق)؛ الثنائيات مُصعَّدة للرفع عبر المتصفح، والموقع
  يعمل ببدائل خطوط النظام حتى تصل.
- axe-core 4.14.0: 0 مخالفة، 27 اجتيازًا، باللغتين. قياس
  Lighthouse (≥90 وصولًا وأداءً) لم يُجرَ (انظر «غير موثّق»).

## 10. اللقطات والتوثيق والأرشيف (المرحلة 10)

- 10 لقطات من بنيات حقيقية عاملة: النافذة الرئيسية بأربع
  تركيبات (داكن/عربي، فاتح/إنجليزي، داكن/إنجليزي،
  فاتح/عربي)، ونافذة الإضافة ببيانات HLS وDASH من ملقّم محلي،
  ونافذة تقدم التنزيل مع شريط المقاطع، ولقطتا الموقع (عربي
  فاتح، إنجليزي داكن). كل لقطة فُحصت بصريًا؛ القائمة الكاملة
  مع البصمات في `docs/RELEASE-REPORT.md` §10. اللقطات ثنائيات
  لا يدفعها الموصل — تُرفَع عبر المتصفح.
- هذا الملف، و`docs/RELEASE-REPORT.md` (تقرير §4 النهائي لكل
  مرحلة)، وأرشيف `ezoo-download-1.1.0-enhanced.zip`.
- لقطات ويندوز لم تُلتقَط (لا آلة ويندوز ولا تشغيل CI متاحًا
  لهذه المرحلة)؛ تعديل صغير غير مختبَر ومأمون (خطوة
  `continue-on-error` + رفع مصنوعات) مُصعَّد للمنسّق في
  `~/workspace/ezoo/workflow-escalations/phase10/`.

## 11. نقاط القوة / ما أُصلح في 1.1.0 / ما هو مخطط له لاحقًا

**نقاط القوة في 1.1.0:**

- **خطوط مضمّنة بلا اعتماد على النظام**: Carlito وNoto Sans Arabic
  وLiberation Serif (SIL OFL 1.1، تُشحَن دون تعديل)، تُسجَّل في
  الذاكرة عند بدء التشغيل، فتعمل الواجهة بالكامل من الخطوط
  المضمّنة (أُثبت بلقطات متطابقة البكسل مع صفر خطوط نظام قابلة
  للاستخدام، بالاختبار البديل الموثّق في المرحلة 3).
- **إصلاح حوارات Wayland بالتحميل الكسول**: الحوارات تُنشَأ عند أول
  فتح فقط، فلا تُعيَّن إلا نافذة واحدة عند بدء التشغيل (أُثبت تحت
  Xvfb: نافذة واحدة عند البدء، ونافذة واحدة لكل حوار يُفتَح،
  وإعادة استخدام عند إعادة الفتح)؛ التحقق على KDE Plasma
  الحقيقي ما زال معلّقًا (انظر «مخطط له لاحقًا»).
- **محرك تجزئة تكيفي مع تعلّم لكل مضيف**: تعلّم محلي فقط في
  `settings.json` (متوسطات أسّية بوزن 0.3، وانتهاء بعد 30 يومًا،
  والمفتاح اسم المضيف فقط)، بلا أي حركة شبكة إضافية.
- **فاحص تحديثات موقَّع**: مصدر وحيد هو إصدارات المستودع الفرعي،
  وتوقيع Ed25519 على البيان قبل أي عرض، ومطابقة SHA-256 مزدوجة،
  وذاكرة ETag، وقابل للإيقاف من الخيارات.
- **رفض DRM بلا التفاف**: البثوث المحمية تُرفَض مبكرًا، وAES-128
  ما زال مدعومًا.
- **لا تتبّع إطلاقًا**: لا قياس ولا تحليلات ولا تقارير أعطال في
  أي مكان (انظر `docs/PRIVACY.md`)، وكل طلبات الشبكة الصادرة
  مخزونة هناك.
- **تحضير Flatpak**: بناء دون اتصال بمصادر مُورَّدة (709/709
  حزمة تطابق `Cargo.lock`)، وبأذونات دنيا فقط (لا `home` ولا
  `host`)؛ البناء والتشغيل الفعليان بانتظار CI.
- **موقع ثنائي اللغة**: عربي RTL افتراضيًا وإنجليزي LTR، بلا
  إطار وبلا متتبّعات ولا نصوص طرف ثالث، وaxe-core: 0 مخالفة.

**ما أُصلح في 1.1.0:**

- تَخطيط الحوارات جنبًا إلى جنب كألسنة مسطّحة على Wayland
  (المرحلة 6، بالتحميل الكسول)؛ التحقق الحقيقي على KDE Plasma
  ما زال معلّقًا.
- إزالة الخط غير المرخّص (InstaArabi) والملفات المُعاد تسميتها
  (المرحلة 3).
- إزالة واجهات التبرّع والترويج مع بقاء كل الإشادات
  (المرحلة 2).
- دقة حجم الملف في نافذة الالتقاط: فحص HEAD أولًا ثم تأكيد عبر
  GET جزئي، وتقديرات البث موسومة دائمًا كمقدَّرة مع مصدرها
  (المرحلة 4).
- تصحيح مصدر التحديثات إلى المستودع الفرعي
  (`kerlif/ezoo-download`) بدل المستودع الأصلي (المرحلة 7).

**مخطط له لإصدارات لاحقة (ليس في 1.1.0):**

- التحقق الحقيقي على KDE Plasma (Wayland) لإصلاح الحوارات
  (سؤال مفتوح 2).
- لقطات ويندوز (سؤال مفتوح 15).
- قياس Lighthouse للموقع الحي (سؤال مفتوح 14).
- مصنوع Flatpak من CI (المرحلة 8 بانتظار CI).
- توجيه محدِّد الجودة للتنزيل فعلًا بدل العرض فقط
  (سؤال مفتوح 7).
- إعادة تصميم واجهة الإضافة: زر عائم على طراز IDM «Download
  this video (N)» يظهر على الفيديوهات، أجمل وأوضح، ويُبرز
  الملفات الملتقَطة مع جودتها وحجمها (سؤال مفتوح 16؛ لا شيفرة
  في 1.1.0).

## قيود معروفة

1. **محدِّد الجودة للعرض فقط** (المرحلة 4، سؤال مفتوح 7):
   اختيار جودة لا يوجّه التنزيل؛ المحرك ينزّل أفضل جودة كما
   قبل. التوجيه يحتاج سباكة جديدة عبر `NewDownload` ومسارَي
   HLS/DASH.
2. **بصمة AppImage تُقاطَع ولا تُوقَّع مباشرة** (سؤال 12):
   توقيع `version.json` يغطي الإصدار وبصمة مثبّت ويندوز فقط؛
   AppImage يُتحقَّق منه مقابل `SHA256SUMS.txt`، والبيان الموقَّع
   يَحجُب العرض ككل. ملف المجاميع نفسه غير موقَّع.
3. **ذعر fontique بصفر خطوط** (سؤال 5): fontique 0.11.1 يذعر
   عند تهيئة منصة Slint إذا لم يرَ fontconfig أي خط؛ لا شيفرة
   تطبيق يمكنها تجنّبه. الأنظمة الحقيقية تشحن خطًا واحدًا على
   الأقل، والاختبار البديل (خط واحد بلا تغطية) أثبت العرض
   المضمّن بالكامل.
4. **قراءة المقياس «جرّب ثم قِس»** (سؤال 9): المقياس يضيف
   اتصالًا ثم يحكم على النافذة التالية؛ القراءة الحرفية
   «انتظر ارتفاعًا قبل الإضافة» لن تنمو أبدًا على خادم يحدّ
   كل اتصال (أُثبت بالمعيار الأول: 12.0 ث مقابل 5.8 ث
   لثابت-4).
5. **ازدواج عدّ محتمل لبتّات الصوت في HLS** (سؤال 8): إذا كان
   وسم `BANDWIDTH` يشمل الصوت أصلًا، فإضافة بتّات الصوت
   المقاسة ترفع التقدير قليلًا.
6. **ترجمات موسومة `needs-review`**: عبارتا تقدير الحجم
   (الروسية/التركية)، و«Stalled» ووسم الشريط (اللغات الست)،
   ونصوص المحدِّث الخمسة — صياغات اجتهادية قصيرة بانتظار مراجعة
   ناطقين أصليين (أسئلة 6 و10 و11).
7. **تدهور Flatpak المُصمَّم**: لا أصوات أحداث (لا `paplay` في
  الصندوق)، والتشغيل التلقائي داخل الصندوق بلا أثر على دخول
  المضيف، وجسر الإضافة يعتمد على مشاركة فضاء الشبكة — كلها
  موثّقة في `docs/FLATPAK.md` وغير مختبَرة بتشغيل حقيقي.

## عناصر غير موثّقة

- التحقق على KDE Plasma (Wayland) الحقيقي لإصلاح الحوارات
  (سؤال 2)؛ ولون «متوقف» في شريط المقاطع لم يُرَ حيًا (غطّاه
  اختبار وحدة فقط).
- تجميع هدف ويندوز محليًا (لا أدوات mingw) — بما فيها مسارات
  ويندوز فقط في المحدِّث والخطوط؛ يجب أن يؤكده CI (خطوتا
  clippy والاختبارات الجديدة في سير العمل المُصعَّد).
- بناء/تشغيل Flatpak، ووجود ثنائي `ffmpeg` في الصندوق، ودمج
  حقيقي واحد، وجسر المتصفح من متصفح مضيف، ومدققا
  `appstreamcli`/`desktop-file-validate` — كلها بانتظار CI
  وتشغيل حقيقي (المرحلة 8).
- قياسات Lighthouse للموقع الحي (سؤال 14)؛ ولقطات ويندوز
  (سؤال 15)؛ وسلوك الشبكة الحية (كل الفحوص بملقّمات محلية)؛
  والتعلّم لكل مضيف عبر جولات متعددة؛ وبناءا `.rpm`/AppImage
  في المرحلة 3 (فُحص `.deb` حقيقي فقط)؛ واختبارات واجهة
  رسومية حيث سُجّل تخطّيها في المراحل 4–7 (عُملت لاحقًا في
  المرحلتين 6 و10 تحت Xvfb).
- الثنائيات المعلّقة: 6 ملفات `.ttf` (المرحلة 3) + 4 ملفات
  `.woff2` (المرحلة 9) + 10 لقطات PNG (المرحلة 10) ليست على
  البعيد بعد — البناء من الوسم سيفشل دون الخطوط، والموقع
  سيعرض اللقطات القديمة دون اللقطات الجديدة.

---

# Improvements Summary — Ezoo Download 1.1.0

> Branch: `v110-upgrade` on the fork `kerlif/ezoo-download`.
> License: MIT © 2026 Ahmed Ezzat (Ezoo Tech). All copyright
> notices and credits to the original author and the upstream
> repository `AhmedEzo686/ezoo-download` are kept; no license
> terms changed. This file summarizes what upgrade phases 0–10
> actually did, per the phase reports in `docs/`; no new claims
> are made here.

## 1. Safe cleanup (Phase 1)

- Whole-tree `cargo fmt`, all `clippy` warnings fixed (including
  Windows-only paths via `#[cfg(windows)]`), Slint deprecations
  renamed.
- Version 1.1.0 in code and packaging. No behavior change.
- Tests: 83 passed / 0 failed (as at baseline).

## 2. Promotional and donation UI removed (Phase 2)

- Donation blocks removed from the About and What's New dialogs,
  the YouTube/Facebook channel buttons removed from About with
  their `open-link` handlers, 6 translation keys, and the
  InstaPay QR image.
- All credits kept: "Coded by Ahmed Ezzat", upstream links, and
  the installer publisher metadata (not user-visible promo UI).
- `.po` files regenerated (473 → 468 texts, 0 missing).

## 3. Bundled fonts (Phase 3)

- InstaArabi removed (no published license: name table reads
  "All Rights Reserved", no license text) and the renamed
  "Times New Roman" files removed (in fact Liberation Serif under
  an impersonated family name).
- New fonts, all SIL OFL 1.1, shipped unmodified: Carlito
  (400/700) for Latin, Noto Sans Arabic (400/700, v2.013) for
  Arabic, Liberation Serif (400/700) as document-style choice
  only.
- In-memory registration at startup through Slint's shared
  fontique collection, with per-script fallbacks (Arabic → Noto
  Sans Arabic, Latin → Carlito); no more writing fonts to the
  user's font folder; legacy installed copies deleted; saved
  settings migrate automatically.
- Proven by test that the UI renders entirely from bundled fonts
  (pixel-identical screenshots with zero usable system fonts).
  Documented exception: the literal zero-font test fails inside
  fontique itself before any app code runs (see Known
  limitations).

## 4. File size and stream metadata in the capture window (Phase 4)

- HEAD-first probe (Content-Length), confirmed by a ranged GET
  (`Content-Range`); a lying HEAD length is corrected, an
  unconfirmed length is flagged unverified and shown as an
  estimate.
- New `stream_meta` module: HLS master/media parsing
  (`AVERAGE-BANDWIDTH` preferred; separate-audio-group bitrate
  added from sampled segments), multi-Period DASH parsing
  (durations summed, video/audio kept separate), size =
  (video+audio bps) × duration / 8, always labeled an estimate
  with its source. ~5 s probe budget off the UI thread.
- The Add dialog now shows duration, a quality selector with
  every variant, and the estimated size of the selected quality.
  **The quality selector is display-only**: selection does not
  steer the download yet (see Known limitations).
- DRM refusal: Widevine/PlayReady/FairPlay scheme UUIDs and HLS
  DRM KEYFORMATs disable the buttons and refuse the download
  early; AES-128 stays supported. No DRM circumvention.
- Filename hardening (`..` stripped, `safe_join` verified inside
  the download dir) and yt-dlp size shown in the browser panel.
- Tests: 105 passed / 0 failed (19 new, local-server fixtures).

## 5. Adaptive segmentation engine — clean room (Phase 5)

Designed from first principles in the author's own words (see
`docs/ENGINE.md`); no outside code copied or studied, and no
other product is named in UI or website copy.

- **Connection scaler**: starts at 2–4 connections, then probes
  by adding one and judging the next 3 s window: ≥10% rise →
  probe again; fall → remove one; flat for two windows → stop
  growing. Caps 16 per host, 32 absolute. Removal is graceful
  (no connection killed mid-segment).
- **Backoff**: 429/503 with `Retry-After` parsing (seconds or
  HTTP date, clamped to 300 s) + up to 25% jitter; three
  consecutive resets step the count down. Rate limiting costs no
  retry.
- **Dynamic splitting**: an idle worker cuts the largest active
  segment at the midpoint of what remains (≥1 MiB left) under one
  lock, with no gaps or overlaps, and the segment table persisted
  before any byte is written.
- **Stall rescue**: per-segment monitor (baseline seeded from
  the segment's first bytes; reference = the higher of its own
  baseline and its peers' mean); a segment below 15% of the
  reference for 10 consecutive seconds (after a 10 s grace) is
  flagged for takeover and re-requested from the last byte
  written — no data lost.
- **Per-host learning**: local only, in `settings.json`
  (`host_learning`), exponential averages (weight 0.3), 30-day
  expiry, keyed by host name only; no network traffic at all.
  User-set per-server exceptions remain caps.
- **Resume validation**: `validate_segment_map` rejects any
  segment table with a gap, overlap, or out-of-bounds entry and
  re-plans from scratch. No Range support or unknown size: one
  connection, as before.
- **Benchmarks** (local server, 3 runs per config, SHA-256
  verified every run — see `docs/BENCHMARKS.md`): on an 8 MiB
  per-connection-capped file, adaptive 4.315 s mean (peak 10
  connections) vs fixed-1: 23.367, fixed-4: 5.824, fixed-8:
  2.904, fixed-16: 1.466. On a fast 32 MiB file, adaptive 1.797 s
  (peak 7) vs fixed-1: 8.589, fixed-4: 2.207, fixed-8: 1.110,
  fixed-16: 0.556. Adaptive beats a too-low fixed count and loses
  to well-chosen fixed-8/16 — reported honestly. Cross-run
  learning was not measured.
- No new dependencies (sha2 is a dev-dependency only). Tests: 133
  passed / 0 failed.

### Tunable engine parameters (`AdaptiveConfig`)

| Setting | Default |
|---|---|
| `initial_connections` / `min_initial_connections` | 4 / 2 |
| `measure_window` / `gain_threshold` / `no_gain_intervals` | 3 s / 1.10 / 2 |
| `per_host_max` / `absolute_ceiling` | 16 / 32 |
| `dynamic_split_min_remaining` | 1 MiB |
| `stall_fraction` / `stall_seconds` / `stall_grace` | 0.15 / 10 s / 10 s |
| `rate_limit_default_backoff` / `max_retry_after` / `jitter_fraction` | 5 s / 300 s / 0.25 |
| `learning_alpha` / `learning_expiry_days` / `reset_reduction_after` | 0.3 / 30 days / 3 |

## 6. UI, dark mode, and the Wayland dialog fix (Phase 6)

- **Wayland fix**: every dialog is now created lazily on first
  open (`LazyWeak` + `ensure_*`) instead of ~20 windows at
  startup. Root cause documented: the Slint/winit backend
  creates every inactive window, and Wayland ignores "invisible",
  so ~20 toplevels mapped at once as flat side-by-side tabs.
  Proven fixed under Xvfb (one window at startup, one per opened
  dialog, reuse on re-open); real KDE Plasma (Wayland)
  verification is still pending.
- Segment bar in the Details dialog: per-connection colored
  spans (downloading / done / stalled / idle) with a legend,
  read-only, no engine behavior change.
- Toolbar overflow fixed (927 px ≤ 960 px default) and a color
  audit: every text/background pair in all 9 themes is ≥4.5:1
  (see `docs/THEMES.md`); Dark values unchanged.
- Tests: 134 passed / 0 failed.

## 7. Update checker (Phase 7) — security model

- Single source: the fork's releases (`kerlif/ezoo-download`)
  from one `REPO` constant; no second checker.
- Sequence: releases API announcement → signed `version.json`
  manifest → `SHA256SUMS` file, all with `User-Agent:
  EzooDownload/<version>` only, an ETag cache
  (`checker-cache.json`), and hand-followed redirects (max 3,
  same-origin or GitHub asset hosts only).
- Verification before anything is offered: Ed25519 (`ring`)
  signature over the manifest covering the version and the
  Windows installer's SHA-256; then dual SHA-256 matching
  (signed manifest + checksum file) for the downloaded artifact,
  re-checked immediately before install. An unsigned manifest is
  never offered.
- Scheduling: first check 3 s after startup, then every 24 h,
  with a backoff ladder (5m/15m/1h/6h/24h) and `Retry-After`
  winning (clamped 1m–24h); all failures silent. Can be turned
  off in Options > General (on by default).
- Platform behavior: installed Windows copies ask before
  installing and wait for downloads to idle; AppImages are
  replaced atomically after verification and asking; .deb/.rpm/
  Flatpak/unknown copies are notify-only with a release-page
  link. Portable copies are notify-only.
- A new fork Ed25519 keypair (public key committed in
  `selfupdate.rs`, private key outside the tree, mode 600, and
  the `UPDATE_SIGNING_KEY` CI secret still to be created in the
  browser). Until it exists, `version.json` ships with an empty
  `sig` and the in-app updater refuses it (manual install works).
- Tests: 156 passed / 0 failed (22 new, 13 of them local-server
  integration covering offline/rate-limit/bad-signature/
  cross-host-redirect/ETag cases).

## 8. Flatpak and CI (Phase 8 — prepared, awaiting CI)

- App ID `io.github.kerlif.EzooDownload`, Freedesktop runtime
  26.08, fully offline build with vendored sources (709/709
  crates, all hashes matching `Cargo.lock`).
- Permissions: `dri`, `ipc`, `network`, Wayland/fallback-X11,
  and `xdg-download` only (no `home`, no `host`), plus the file
  chooser portal and the StatusNotifier name for the tray.
- No `ffmpeg-full` (retired for 25.08+); the `codecs-extra`
  extension installs automatically with the runtime — whether an
  `ffmpeg` binary is on the sandbox `PATH` is unverified (open
  question 13).
- yt-dlp: no fixed copy shipped; the existing hash-checked
  automatic fetch into the sandbox data directory is the supply
  (open question 1, resolved).
- CI workflows escalated via the browser (the connector cannot
  push `.github/workflows/*`): Flatpak built first in the Linux
  job, the `.flatpak` reaching the release through `dist/*` and
  `SHA256SUMS.txt`, Windows clippy + full tests, and a hard error
  when the signing key is missing on releases. Tag `v1.1.0` not
  created yet, no CI job has run — Phase 8 is not "passed" until
  the CI verification follow-up reports.

## 9. Website redesign (Phase 9)

- Framework-free rebuild (vanilla HTML/CSS/JS, zero build step,
  no trackers, no third-party scripts, no external fonts) in
  `docs/`.
- Bilingual (Arabic RTL by default, English LTR) with a 176-key
  dictionary swapping all strings plus `lang`/`dir`/title,
  persisted in `localStorage`; light + dark themes (all computed
  contrast pairs ≥4.5:1), skip link, landmarks, visible focus,
  keyboard operation.
- Sections: hero, features, how-it-works (with the preserved
  interactive simulator: slider, size select, lanes, chart,
  single-connection comparison), downloads with OS detection,
  install guides (Windows/.deb/.rpm/AppImage/Flatpak/extension)
  with copyable commands and hardcoded v1.1.0 filenames, FAQ
  (verified answers only, including the display-only quality
  selector and DRM refusal), privacy summary, changelog link.
- Four WOFF2 site fonts generated locally from the Phase 3
  originals (identical glyph counts); binaries escalated for
  browser upload, site renders on system-font fallbacks until
  they land.
- axe-core 4.14.0: 0 violations, 27 passes, both languages.
  Lighthouse (≥90 accessibility + performance) not measured
  (see Unverified).

## 10. Screenshots, documentation, and archive (Phase 10)

- 10 screenshots from real running builds: the main window in
  four combinations (dark/Arabic, light/English, dark/English,
  light/Arabic), the Add dialog with HLS and DASH metadata from a
  local fixture server, the download-progress window with the
  segment bar, and the redesigned website (Arabic light, English
  dark). Every shot was inspected visually; the full list with
  hashes is in `docs/RELEASE-REPORT.md` §10. Screenshots are
  binaries the connector cannot push — they are escalated for
  browser upload.
- This file, `docs/RELEASE-REPORT.md` (the §4 final report for
  every phase), and the archive
  `ezoo-download-1.1.0-enhanced.zip`.
- Windows screenshots were not captured (no Windows machine or
  CI run was available to this phase); a small untested
  best-effort amendment (`continue-on-error` step + artifact
  upload) is staged for the coordinator at
  `~/workspace/ezoo/workflow-escalations/phase10/`.

## 11. Strengths, fixes, and later work

**Strengths of 1.1.0:**

- **Bundled fonts, no system dependency**: Carlito, Noto Sans Arabic,
  and Liberation Serif (SIL OFL 1.1, shipped unmodified), registered
  in memory at startup, so the UI renders entirely from bundled fonts
  (proven by pixel-identical screenshots with zero usable system
  fonts, via the documented substitute test in Phase 3).
- **Lazy-dialog Wayland fix**: dialogs are created only on first open,
  so exactly one window maps at startup (proven under Xvfb: one window
  at startup, one per opened dialog, reuse on re-open); real KDE
  Plasma verification is still pending (see "Planned for later").
- **Adaptive segmentation engine with per-host learning**: learning is
  local only, in `settings.json` (exponential averages, weight 0.3,
  30-day expiry, keyed by host name only), with no extra network
  traffic of any kind.
- **Signed update checker**: a single source — the fork's releases —
  Ed25519 signature verification of the manifest before anything is
  offered, dual SHA-256 matching, an ETag cache, and an opt-out in
  Options.
- **DRM refusal, no circumvention**: DRM-signaled streams are refused
  early; AES-128 stays supported.
- **No telemetry**: no analytics, no crash reporting anywhere (see
  `docs/PRIVACY.md`); every outbound request is inventoried there.
- **Flatpak preparation**: fully offline build with vendored sources
  (709/709 crates matching `Cargo.lock`), minimal permissions only
  (no `home`, no `host`); the actual build and run await CI.
- **Bilingual website**: Arabic RTL by default and English LTR,
  framework-free, no trackers and no third-party scripts, axe-core:
  0 violations.

**Fixed in 1.1.0:**

- The side-by-side flat-tab dialog layout on Wayland (Phase 6, lazy
  creation); real KDE Plasma verification is still pending.
- Removal of the unlicensed font (InstaArabi) and the misleadingly
  renamed files (Phase 3).
- Removal of the donation and promotional UI, with all credits kept
  (Phase 2).
- Capture-window file-size accuracy: HEAD-first probing with ranged-GET
  confirmation, and stream estimates always labeled as estimates with
  their source (Phase 4).
- Update-source correction to the fork (`kerlif/ezoo-download`)
  instead of the upstream repository (Phase 7).

**Planned for later releases (not in 1.1.0):**

- Real KDE Plasma (Wayland) verification of the dialog fix (open
  question 2).
- Windows screenshots (open question 15).
- Lighthouse measurement of the live site (open question 14).
- A Flatpak CI artifact (Phase 8 awaits CI).
- Quality-selector steering of downloads, instead of display-only
  (open question 7).
- Extension-UI redesign: an IDM-style floating "Download this video
  (N)" overlay button appearing on videos, more aesthetic and clearer,
  highlighting the captured files with their quality and size (open
  question 16; no code in 1.1.0).

## Known limitations

1. **Display-only quality selector** (Phase 4, open question 7):
   choosing a quality does not steer the download; the engine
   still downloads the best quality. Steering needs new plumbing
   through `NewDownload` and the HLS/DASH runners.
2. **AppImage hash is cross-checked, not directly signed**
   (open question 12): the `version.json` signature covers the
   version and the Windows installer's hash only; the AppImage is
   verified against `SHA256SUMS.txt`, and the signed manifest
   gates the whole offer. The checksum file itself is unsigned.
3. **fontique zero-font panic** (open question 5): fontique
   0.11.1 panics during Slint platform init if fontconfig sees
   zero fonts; no app code can avoid it. Real systems ship at
   least one font, and the substitute test (one font, zero UI
   coverage) proved fully bundled rendering.
4. **Probe-then-measure scaler reading** (open question 9): the
   scaler adds a connection and then judges the next window; the
   literal "wait for a rise before adding" reading would never
   grow on a per-connection-capped server (proven by the first
   benchmark: 12.0 s vs 5.8 s for fixed-4).
5. **Possible HLS audio-bitrate double count** (open question 8):
   if a `BANDWIDTH` tag already includes audio, adding the
   sampled audio bitrate makes the estimate slightly high.
6. **`needs-review` translations**: the size-estimation phrases
   (Russian/Turkish), "Stalled" and the segment-bar legend (six
   locales), and the five update-checker strings — best-effort
   wordings awaiting native-speaker review (open questions 6, 10,
   11).
7. **Designed Flatpak degradations**: no event sounds (no
   `paplay` in the sandbox), autostart inside the sandbox has no
   effect on host login, and the extension bridge relies on the
   shared network namespace — all documented in
   `docs/FLATPAK.md` and untested in a real run.

## Unverified items

- Real KDE Plasma (Wayland) verification of the dialog fix
  (open question 2); the "stalled" segment-bar color was never
  seen live (unit test only).
- Windows-target compile locally (no mingw toolchain) —
  including the Windows-only updater and font paths; CI must
  confirm (new clippy + full-test steps in the escalated
  workflow).
- Flatpak build/run, `ffmpeg` binary presence in the sandbox,
  one real merge, the host-browser bridge, `appstreamcli` /
  `desktop-file-validate` — all awaiting CI / a real run
  (Phase 8).
- Lighthouse scores for the live site (open question 14);
  Windows screenshots (open question 15); live-network behavior
  (all probing/benchmark checks used local servers); per-host
  learning across runs; `.rpm`/AppImage builds in Phase 3 (a
  real `.deb` listing only was checked); GUI smoke tests where
  phases recorded them skipped (later covered under Xvfb in
  Phases 6 and 10).
- Pending binaries: 6 `.ttf` files (Phase 3) + 4 `.woff2` files
  (Phase 9) + 10 screenshot PNGs (Phase 10) are not on the
  remote yet — a tag build fails without the fonts, and the site
  shows the old screenshots until the PNGs land.
