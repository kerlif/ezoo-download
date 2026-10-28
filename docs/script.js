'use strict';
/* Ezoo Download project page — interactions. Original code, vanilla JS, no dependencies.
   Bilingual (Arabic RTL default / English LTR), light + dark themes, OS detection.
   The download simulator below is a simulation only — no real download happens. */
document.documentElement.classList.add('js');

/* Hooks set by the simulator IIFE below; declared up here so the
   language/theme functions can reference them before that IIFE runs. */
let drawChartExternal = null;
let refreshSimulatorText = null;

/* ------------------------------------------------------------------ i18n */
const I18N = {
  ar: {
    'meta.title': 'Ezoo Download — مدير تنزيلات مجاني ومفتوح المصدر',
    'meta.description': 'Ezoo Download: مدير تنزيلات سريع، مجاني، ومفتوح المصدر لويندوز ولينكس. اتصالات تكيفية، استئناف التنزيل، إضافة للمتصفح، وffmpeg مدمج.',
    'a11y.skip': 'تخطَّ إلى المحتوى الرئيسي',
    'brand.alt': 'Ezoo Download',
    'nav.features': 'المزايا', 'nav.how': 'كيف يعمل', 'nav.downloads': 'تنزيل', 'nav.install': 'التثبيت',
    'nav.screenshots': 'لقطات', 'nav.faq': 'الأسئلة', 'nav.privacy': 'الخصوصية',
    'theme.dark': 'داكن', 'theme.light': 'فاتح',
    'hero.badge': 'الإصدار 1.1.0  •  مجاني 100%  •  مفتوح المصدر',
    'hero.title': 'مدير تنزيلات سريع،<br />مجاني، ومفتوح المصدر.',
    'hero.sub': 'اتصالات تتكيف مع ما يقدمه الخادم فعلًا، وحجم وجودة ظاهران قبل التأكيد، والتقاط الروابط من المتصفح بضغطة واحدة — على ويندوز ولينكس، دون اشتراكات ودون إعلانات.',
    'hero.primary': '⬇ تنزيل Ezoo Download',
    'hero.secondary': 'كل الإصدارات وسجل التغييرات',
    'hero.note': 'نسخ لينكس تتضمن ffmpeg مدمجًا — لا حاجة لتثبيت أي شيء إضافي. تحقق من البصمات في <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS.txt">SHA256SUMS.txt</a>.',
    'hero.shot.alt': 'النافذة الرئيسية لبرنامج Ezoo Download: قائمة التنزيلات والتصنيفات وشريط الأدوات',
    'strip.1t': 'مجاني 100%', 'strip.1s': 'دون إعلانات أو اشتراكات',
    'strip.2t': 'مفتوح المصدر', 'strip.2s': 'مكتوب بلغة Rust',
    'strip.3t': 'ويندوز + لينكس', 'strip.3s': 'Setup.exe وdeb وAppImage وrpm وFlatpak',
    'strip.4t': '8 لغات', 'strip.4s': 'منها العربية',
    'features.title': 'كل ما تحتاجه من مدير تنزيلات',
    'features.sub': 'واجهة بسيطة، وأداء يتكيف مع اتصالك والخادم الذي تحمّل منه.',
    'f1t': 'اتصالات تكيفية',
    'f1d': 'يقيس المحرك ما يقدمه الخادم فعلًا ويعدّل عدد الاتصالات أثناء التنزيل، ويتذكر أفضل عدد لكل مضيف محليًا فقط، وينقذ المقاطع المتوقفة باتصال جديد.',
    'f2t': 'الحجم والجودة قبل التأكيد',
    'f2d': 'نافذة التقاط الرابط تفحص الرابط أولًا وتُظهر المدة والجودات المتاحة وحجمًا تقديريًا (من البثوث HLS وDASH أيضًا)، وترفض البثوث المحمية بنظام DRM.',
    'f3t': 'التقاط من المتصفح',
    'f3d': 'إضافة كروم وفايرفوكس ترسل الروابط إلى البرنامج عبر جسر محلي فقط (127.0.0.1:9614) — بضغطة واحدة من الصفحة، ودون أي خادم وسيط.',
    'f4t': 'استئناف حقيقي',
    'f4d': 'أوقف التنزيل وأغلق البرنامج، وسيكمل من حيث توقف عند عودتك. خريطة المقاطع محفوظة، وأي خريطة تالفة تُهمل ويُعاد التخطيط من الصفر.',
    'f5t': 'فيديو وصوت',
    'f5d': 'من الصفحات وقوائم التشغيل عبر yt-dlp — ولا يعمل إلا عندما تطلبه أنت — مع دمج الفيديو والصوت بـ ffmpeg المدمج في حزم لينكس.',
    'f6t': 'جدولة وقوائم انتظار',
    'f6d': 'شغّل التنزيلات في موعد محدد، وأوقف الجهاز عند الانتهاء، ورتّب التنزيلات في قوائم انتظار.',
    'f7t': 'تنظيم وتحديد سرعة',
    'f7d': 'كل ملف يذهب إلى مجلده حسب نوعه تلقائيًا، مع حدّ عام للسرعة أو حدّ خاص بكل ملف، و9 سمات منها الداكنة.',
    'f8t': 'خصوصيتك أولًا',
    'f8d': 'لا قياس عن بُعد ولا تحليلات ولا تقارير أعطال. الطلبات الصادرة الوحيدة هي تلك المذكورة في قسم الخصوصية أدناه — ولا شيء غيرها.',
    'how.title': 'كيف يعمل؟ ثلاث خطوات فقط',
    's1t': 'انسخ الرابط',
    's1d': 'يلتقطه البرنامج تلقائيًا من الحافظة، أو أضِفه بنفسك — أو أرسله من إضافة المتصفح.',
    's2t': 'يقسّمه على اتصالات تكيفية',
    's2d': 'يبدأ بعدد قليل من الاتصالات ويزيدها ما دامت السرعة ترتفع، ويقسّم المقاطع الكبيرة أثناء التنزيل ليصل إلى أقصى سرعة ممكنة.',
    's3t': 'استأنف متى شئت',
    's3d': 'انقطع الاتصال؟ أغلقت الجهاز؟ يكمل التنزيل من حيث توقف دون أن يبدأ من جديد.',
    'sim.title': 'جرّب قوة التقسيم بنفسك',
    'sim.sub': 'محاكاة توضيحية تعمل داخل متصفحك — ليست تنزيلًا حقيقيًا. تبدأ تلقائيًا عندما تصل إليها بالإعدادات الافتراضية (8 اتصالات)، ويمكنك تغيير العدد والحجم وإعادة تشغيلها.',
    'sim.note': 'محاكاة فقط — لا يجري أي تنزيل حقيقي هنا.',
    'sim.threads': 'عدد الاتصالات:', 'sim.threads-hint': '1 ← اتصال واحد · 16 ← أقصى تقسيم',
    'sim.size': 'حجم الملف',
    'sim.size.100': '100 ميجابايت', 'sim.size.500': '500 ميجابايت', 'sim.size.1024': '1 جيجابايت', 'sim.size.2560': '2.5 جيجابايت',
    'sim.start': '▶ ابدأ المحاكاة', 'sim.pause': '⏸ إيقاف مؤقت', 'sim.resume': '▶ استئناف', 'sim.reset': '↺ إعادة',
    'stat.speed': 'ميجابايت/ث', 'stat.progress': 'التقدم', 'stat.eta': 'الوقت المتبقي', 'stat.active': 'اتصالات نشطة',
    'chart.aria': 'رسم بياني لسرعة التنزيل أثناء المحاكاة', 'chart.label': 'السرعة عبر الزمن',
    'sim.lane': 'اتصال', 'sim.done': '✓ اكتمل', 'sim.sec': 'ث',
    'sim.result': 'اكتمل التنزيل (محاكاة) في {secs} ثانية بمتوسط {avg} ميجابايت/ث — باتصال واحد كان سيستغرق نحو {single} ثانية بنفس الظروف، أي أسرع بنحو {faster}× بفضل التقسيم.',
    'dl.title': 'نزّل Ezoo Download 1.1.0',
    'dl.sub': 'اختر نظامك — كل الملفات من صفحة إصدارات هذا المستودع، وكل الروابط تشير إلى أحدث إصدار.',
    'os.unknown': 'تعذّر تحديد نظام تشغيلك تلقائيًا — كل المنصات مذكورة أدناه.',
    'os.windows': 'تم اكتشاف نظامك: ويندوز — البطاقة المطابقة في الأعلى.',
    'os.linux': 'تم اكتشاف نظامك: لينكس — بطاقة AppImage (تعمل على أي توزيعة) في الأعلى، وبجانبها حزم .deb و.rpm وFlatpak.',
    'os.macos': 'تم اكتشاف نظامك: macOS — لا توجد نسخة macOS من Ezoo Download حاليًا؛ الإصدارات المتاحة لويندوز ولينكس فقط.',
    'os.other': 'نظامك المكتشف لا يطابق منصة مدعومة — Ezoo Download متاح لويندوز ولينكس فقط.',
    'dl.detected': '✓ نظامك',
    'dl.win.t': '🪟 ويندوز 10 / 11', 'dl.win.d': '— ملف إعداد للمستخدم الحالي، دون صلاحيات المسؤول.', 'dl.win.b': 'تنزيل Setup.exe',
    'dl.deb.t': '🐧 لينكس — Debian / Ubuntu', 'dl.deb.d': '— مع ffmpeg مدمج.', 'dl.deb.b': 'تنزيل .deb',
    'dl.app.t': '🐧 لينكس — أي توزيعة', 'dl.app.d': '— دون تثبيت، مع ffmpeg مدمج.', 'dl.app.b': 'تنزيل AppImage',
    'dl.rpm.t': '🐧 لينكس — Fedora / RHEL', 'dl.rpm.d': '— مع ffmpeg مدمج.', 'dl.rpm.b': 'تنزيل .rpm',
    'dl.flat.t': '📦 لينكس — Flatpak', 'dl.flat.d': '— حزمة Flatpak واحدة (معرّف التطبيق io.github.kerlif.EzooDownload).', 'dl.flat.b': 'تنزيل حزمة Flatpak',
    'dl.crx.t': '🌐 إضافة كروم', 'dl.crx.d': '— موقّعة بالمفتاح الأصلي، بنفس معرّف الإضافة.', 'dl.crx.b': 'تنزيل .crx', 'dl.zip.b': 'تنزيل .zip (تحميل يدوي)',
    'dl.xpi.t': '🦊 إضافة فايرفوكس', 'dl.xpi.b': 'تنزيل .xpi',
    'dl.checks': 'تحقق من سلامة الملفات عبر <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS.txt">SHA256SUMS.txt</a> (لينكس) أو <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS-windows.txt">SHA256SUMS-windows.txt</a> (ويندوز) · <a href="https://github.com/kerlif/ezoo-download/releases">كل الإصدارات وسجل التغييرات</a>.',
    'install.title': 'دليل التثبيت',
    'install.sub': 'أوامر دقيقة لكل منصة — انقر «نسخ» ثم الصق في الطرفية.',
    'g.win.t': 'ويندوز 10 / 11 (64-bit)',
    'g.win.1': 'نزّل <code>EzooDownload-Setup.exe</code> من قسم التنزيل أعلاه.',
    'g.win.2': 'شغّل الملف — يُثبَّت للمستخدم الحالي ولا يحتاج إلى صلاحيات المسؤول.',
    'g.win.3': 'بعد التثبيت، افتح البرنامج من قائمة ابدأ. قد تُظهر SmartScreen تحذيرًا لأن البرنامج غير موقّع بشهادة رقمية.',
    'g.deb.t': 'لينكس — .deb (Debian / Ubuntu / Mint)',
    'g.deb.1': 'نزّل ملف <code>.deb</code> ثم ثبّته بالأمر أدناه.',
    'g.deb.2': 'إذا ظهرت رسالة نواقص في التبعيات، أكملها بالأمر الثاني.',
    'g.rpm.t': 'لينكس — .rpm (Fedora / RHEL / openSUSE)',
    'g.rpm.1': 'نزّل ملف <code>.rpm</code> ثم ثبّته بأحد الأمرين أدناه.',
    'g.app.t': 'لينكس — AppImage (أي توزيعة)',
    'g.app.1': 'اجعل الملف قابلًا للتنفيذ ثم شغّله — لا حاجة لأي تثبيت.',
    'g.app.2': 'إذا لم يعمل مباشرة، فغالبًا ينقص نظامك مكوّن FUSE: ثبّت <code>libfuse2</code>، أو فكّ الملف وشغّله من المجلد الناتج.',
    'g.flat.t': 'لينكس — Flatpak',
    'g.flat.1': 'نزّل ملف الحزمة <code>EzooDownload-x86_64.flatpak</code> — هذه الحزمة غير منشورة على Flathub، والتثبيت يكون من ملف الحزمة مباشرة.',
    'g.flat.2': 'ثبّت الحزمة بالأمر أدناه، ثم شغّل البرنامج بالأمر الثاني.',
    'g.ext.t': 'إضافة المتصفح (كروم / فايرفوكس)',
    'g.ext.1': 'كروم (وEdge وBrave وOpera): فعّل <strong>Developer mode</strong> في صفحة <code>chrome://extensions</code>، ثم اسحب ملف <code>.crx</code> وأفلته في الصفحة. إذا عرض كروم تحذيرًا لإضافة غير منشورة في المتجر، اختر <strong>Keep / إبقاء</strong>.',
    'g.ext.2': 'بديل كروم: حمّل ملف <code>.zip</code> غير المعبّأ من <code>chrome://extensions</code> بزر <strong>Load unpacked</strong>.',
    'g.ext.3': 'فايرفوكس: جرّب الإضافة مؤقتًا من <code>about:debugging#/runtime/this-firefox</code> ← <strong>Load Temporary Add-on</strong> ← اختر ملف <code>.xpi</code> (تبقى حتى إغلاق فايرفوكس). التثبيت الدائم يشترط توقيع Mozilla؛ يمكن استخدامها دائمًا على Firefox Developer Edition / Nightly بعد تعطيل <code>xpinstall.signatures.required</code> في <code>about:config</code>.',
    'g.ext.4': 'بعد تثبيت الإضافة، افتح البرنامج وستصل الروابط إليه تلقائيًا عبر الجسر المحلي.',
    'g.ext.note': 'هذا هو معرّف إضافة كروم الثابت — نفس المفتاح يوقّع كل إصدار.',
    'copy.btn': 'نسخ',
    'shots.title': 'ألقِ نظرة',
    'shots.note': 'لقطات من واجهة الإصدار 1.1.0: النافذة الرئيسية، وتقدّم التنزيل، ونافذة الإضافة مع مدة البث وجودته وحجمه المقدّر.',
    'shots.1.alt': 'النافذة الرئيسية لبرنامج Ezoo Download بالعربية: قائمة التنزيلات الفارغة والتصنيفات على الجانب',
    'shots.1.cap': 'النافذة الرئيسية',
    'shots.2.alt': 'نافذة تقدم التنزيل في Ezoo Download بالعربية مع شريط التقدم والسرعة',
    'shots.2.cap': 'تقدّم التنزيل',
    'shots.3.alt': 'نافذة إضافة تحميل جديد في Ezoo Download بالعربية: بث HLS مع المدة والجودة والحجم المقدّر',
    'shots.3.cap': 'إضافة تحميل جديد',
    'faq.title': 'الأسئلة الشائعة',
    'faq.1q': 'هل البرنامج مجاني فعلًا؟',
    'faq.1a': 'نعم — مجاني بالكامل، دون إعلانات، ودون مزايا محجوبة خلف اشتراك.',
    'faq.2q': 'هل الشيفرة مفتوحة المصدر؟',
    'faq.2a': 'نعم. البرنامج مكتوب بلغة Rust، وترخيصه MIT، والشيفرة كاملة في <a href="https://github.com/kerlif/ezoo-download">المستودع</a>.',
    'faq.3q': 'على أي أنظمة يعمل؟',
    'faq.3a': 'ويندوز 10 / 11 (64-bit) ولينكس x86_64: حزم .deb و.rpm وAppImage وFlatpak. لا توجد نسخة macOS.',
    'faq.4q': 'ماذا لو انقطع الاتصال أثناء التنزيل؟',
    'faq.4a': 'يستأنف البرنامج من حيث توقف تلقائيًا، ويمكنك إيقافه واستئنافه يدويًا في أي وقت — حتى بعد إغلاق البرنامج وإعادة فتحه.',
    'faq.5q': 'هل يجمع البرنامج بياناتي؟',
    'faq.5a': 'لا. لا يجمع بيانات استخدام ولا يرسل إحصاءات إلى أي جهة. القائمة الكاملة للطلبات الصادرة موجودة في <a href="#privacy">قسم الخصوصية</a> أدناه.',
    'faq.6q': 'كيف تتم التحديثات؟',
    'faq.6a': 'يفحص البرنامج إصدارات هذا المستودع عند التشغيل وكل 24 ساعة (يمكن إيقاف الفحص من الإعدادات)، ويعرض التحديث فقط بعد التحقق من توقيع ملف version.json وبصمة الملف. نسخ .deb و.rpm وFlatpak تُعلِم فقط ولا تُحدِّث نفسها.',
    'faq.7q': 'هل اختيار جودة في نافذة الالتقاط ينزّل تلك الجودة؟',
    'faq.7a': 'ليس بعد: المحدد يعرض كل الجودات المتاحة مع حجم تقديري لكل منها، لكن التنزيل يأخذ أفضل جودة حاليًا. اختيار الجودة للتنزيل نفسه عملٌ مخطط له لاحقًا.',
    'faq.8q': 'لماذا يرفض البرنامج بعض روابط الفيديو؟',
    'faq.8a': 'البثوث المحمية بأنظمة DRM (مثل Widevine وPlayReady وFairPlay) مرفوضة عمدًا: يمنعها البرنامج ولا يحاول تجاوزها.',
    'privacy.title': 'الخصوصية: ما الذي يتصل به البرنامج فعلًا؟',
    'privacy.intro': 'لا قياس عن بُعد، ولا تحليلات، ولا تقارير أعطال، في أي مكان من البرنامج. هذه هي القائمة الكاملة للطلبات الصادرة — ولا شيء خارجها:',
    'privacy.1': '<strong>فحص التحديثات</strong> (تلقائي، عند التشغيل وكل 24 ساعة، ويمكن إيقافه): طلب واحد إلى <code>api.github.com</code> لمعرفة أحدث إصدار، ثم جلب ملف <code>version.json</code> الموقّع وملف البصمات. الترويسة الوحيدة المُرسلة هي اسم البرنامج وإصداره؛ ولا تُرسل أي بيانات عن المستخدم أو الجهاز.',
    'privacy.2': '<strong>مُحدِّث الأدوات</strong> (تلقائي، مرة يوميًا على الأكثر): يجلب نسخة <code>yt-dlp</code> وملف <code>aria2</code> من إصداراتهما على GitHub ويتحقق من بصمتيهما قبل الاستخدام. لا يُجلب ffmpeg أبدًا — فهو مدمج في الحزمة.',
    'privacy.3': '<strong>التنزيلات نفسها</strong> (بأمرك): تذهب إلى خادم الرابط الذي أضفته فقط، بطلبات <code>Range</code> للتقسيم والاستئناف.',
    'privacy.4': '<strong>فحص الرابط في نافذة الالتقاط</strong> (بأمرك، نحو 5 ثوانٍ لكل مرحلة): طلب <code>HEAD</code> وطلب بايت واحد بنطاق <code>Range</code>، وقراءة قوائم <code>.m3u8</code> / <code>.mpd</code> — كلها إلى خادم الرابط نفسه فقط، ولا يُستدعى أي خدمة بيانات أو صور مصغرة خارجية.',
    'privacy.5': '<strong>تعلّم كل مضيف</strong>: عدد الاتصالات الأفضل ومتوسط حجم المقطع لكل مضيف، محفوظة محليًا فقط في ملف الإعدادات (اسم المضيف فقط، دون روابط أو أسماء ملفات، وتُحذف بعد 30 يومًا). لا يُرسل منها أي شيء.',
    'privacy.6': '<strong>جسر الإضافة</strong>: يستقبل فقط، على <code>127.0.0.1:9614</code> في جهازك، ولا يصدر أي حركة شبكة.',
    'privacy.7': '<strong>yt-dlp</strong>: يعمل فقط عندما تطلب خيارات الفيديو أو قوائم التشغيل أو تحديثه من قائمة Help — ويتصل حينها بمواقع الفيديو نفسها، وهذا سلوك yt-dlp لا سلوك البرنامج.',
    'privacy.link': 'القائمة الكاملة والمفصلة في <a href="https://github.com/kerlif/ezoo-download/blob/main/docs/PRIVACY.md">docs/PRIVACY.md</a>.',
    'license.title': 'الترخيص والشكر',
    'license.body': 'البرنامج مرخّص بترخيص MIT، © 2026 <strong>Ahmed Ezzat</strong> (Ezoo Tech) — المشروع الأصلي في <a href="https://github.com/AhmedEzo686/ezoo-download">AhmedEzo686/ezoo-download</a>. هذه النسخة تطوير منفصل (fork) في <a href="https://github.com/kerlif/ezoo-download">kerlif/ezoo-download</a> أضاف دعم لينكس والاتصالات التكيفية وفحص التحديثات وحزمة Flatpak، مع بقاء كل الحقوق والشكر للمؤلف الأصلي.',
    'license.fonts': 'خطوط هذا الموقع: Carlito وNoto Sans Arabic (ترخيص SIL Open Font License 1.1) — نصا الترخيص في <a href="assets/fonts/OFL-Carlito.txt">OFL-Carlito.txt</a> و<a href="assets/fonts/OFL-NotoSansArabic.txt">OFL-NotoSansArabic.txt</a>. الموقع بلا أطر عمل ولا متتبعات ولا نصوص برمجية من جهات خارجية.',
    'license.changelog': 'سجل التغييرات — صفحة الإصدارات',
    'footer.credits': 'MIT © 2026 Ahmed Ezzat (Ezoo Tech) · تطوير منفصل ودعم لينكس: kerlif · المشروع الأصلي: <a href="https://github.com/AhmedEzo686/ezoo-download">AhmedEzo686/ezoo-download</a>',
    'footer.releases': 'الإصدارات', 'footer.issues': 'الإبلاغ عن مشكلة', 'footer.license': 'الترخيص',
    'copy.copied': '✓ تم النسخ', 'copy.manual': 'انسخ يدويًا'
  },
  en: {
    'meta.title': 'Ezoo Download — a free, open-source download manager',
    'meta.description': 'Ezoo Download: a fast, free, open-source download manager for Windows and Linux. Adaptive connections, resumable downloads, a browser extension, and bundled ffmpeg.',
    'a11y.skip': 'Skip to main content',
    'brand.alt': 'Ezoo Download',
    'nav.features': 'Features', 'nav.how': 'How it works', 'nav.downloads': 'Download', 'nav.install': 'Install',
    'nav.screenshots': 'Screenshots', 'nav.faq': 'FAQ', 'nav.privacy': 'Privacy',
    'theme.dark': 'Dark', 'theme.light': 'Light',
    'hero.badge': 'Version 1.1.0  •  100% free  •  Open source',
    'hero.title': 'A fast, free,<br />open-source download manager.',
    'hero.sub': 'Connections that adapt to what the server actually delivers, size and quality shown before you confirm, and one-click capture from the browser — on Windows and Linux, with no subscriptions and no ads.',
    'hero.primary': '⬇ Download Ezoo Download',
    'hero.secondary': 'All releases & changelog',
    'hero.note': 'Linux builds bundle ffmpeg — nothing extra to install. Verify checksums in <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS.txt">SHA256SUMS.txt</a>.',
    'hero.shot.alt': 'The Ezoo Download main window: the download list, categories, and toolbar',
    'strip.1t': '100% free', 'strip.1s': 'No ads or subscriptions',
    'strip.2t': 'Open source', 'strip.2s': 'Written in Rust',
    'strip.3t': 'Windows + Linux', 'strip.3s': 'Setup.exe, .deb, AppImage, .rpm, Flatpak',
    'strip.4t': '8 languages', 'strip.4s': 'Including Arabic',
    'features.title': 'Everything a download manager needs',
    'features.sub': 'A simple interface, with performance that adapts to your connection and the server you download from.',
    'f1t': 'Adaptive connections',
    'f1d': 'The engine measures what the server actually delivers and adjusts the connection count while downloading, remembers the best count per host locally only, and rescues stalled segments with a fresh connection.',
    'f2t': 'Size & quality before you confirm',
    'f2d': 'The capture window probes the link first and shows duration, available qualities, and an estimated size (for HLS and DASH streams too), and refuses DRM-protected streams.',
    'f3t': 'Browser capture',
    'f3d': 'The Chrome and Firefox extension sends links to the app over a local-only bridge (127.0.0.1:9614) — one click from the page, with no middleman server.',
    'f4t': 'Real resume',
    'f4d': 'Pause a download and close the app — it continues where it stopped when you return. The segment map is persisted, and any corrupt map is discarded and re-planned from scratch.',
    'f5t': 'Video & audio',
    'f5d': 'From pages and playlists via yt-dlp — which only runs when you ask for it — with video+audio merged by the ffmpeg bundled in the Linux packages.',
    'f6t': 'Scheduling & queues',
    'f6d': 'Run downloads at a set time, shut the machine down when finished, and organise downloads into queues.',
    'f7t': 'Organisation & speed limits',
    'f7d': 'Every file goes to its folder by type automatically, with a global speed limit or a per-file limit, and 9 themes including dark ones.',
    'f8t': 'Privacy first',
    'f8d': 'No telemetry, no analytics, no crash reports. The only outbound requests are the ones listed in the Privacy section below — nothing else.',
    'how.title': 'How does it work? Just three steps',
    's1t': 'Copy the link',
    's1d': 'The app picks it up from the clipboard automatically, or add it yourself — or send it from the browser extension.',
    's2t': 'It splits it over adaptive connections',
    's2d': 'It starts with a few connections and adds more while speed keeps rising, splitting large segments mid-download to reach the fastest speed possible.',
    's3t': 'Resume whenever you like',
    's3d': 'Connection dropped? Machine switched off? The download continues where it stopped instead of starting over.',
    'sim.title': 'Try the power of splitting yourself',
    'sim.sub': 'An illustrative simulation that runs inside your browser — not a real download. It auto-starts when you reach it with the defaults (8 connections); you can change the count and size and run it again.',
    'sim.note': 'Simulation only — no real download happens here.',
    'sim.threads': 'Connections:', 'sim.threads-hint': '1 ← a single connection · 16 ← maximum splitting',
    'sim.size': 'File size',
    'sim.size.100': '100 MB', 'sim.size.500': '500 MB', 'sim.size.1024': '1 GB', 'sim.size.2560': '2.5 GB',
    'sim.start': '▶ Start simulation', 'sim.pause': '⏸ Pause', 'sim.resume': '▶ Resume', 'sim.reset': '↺ Reset',
    'stat.speed': 'MB/s', 'stat.progress': 'Progress', 'stat.eta': 'Time left', 'stat.active': 'Active connections',
    'chart.aria': 'Chart of download speed during the simulation', 'chart.label': 'Speed over time',
    'sim.lane': 'Connection', 'sim.done': '✓ Done', 'sim.sec': 's',
    'sim.result': 'Download finished (simulation) in {secs} seconds, averaging {avg} MB/s — a single connection would have taken about {single} seconds under the same conditions, so splitting made it about {faster}× faster.',
    'dl.title': 'Download Ezoo Download 1.1.0',
    'dl.sub': 'Pick your system — every file comes from this repository’s releases page, and every link points at the latest release.',
    'os.unknown': 'Your operating system could not be detected automatically — every platform is listed below.',
    'os.windows': 'Detected OS: Windows — the matching card is first.',
    'os.linux': 'Detected OS: Linux — the AppImage card (works on any distro) is first, alongside the .deb, .rpm, and Flatpak packages.',
    'os.macos': 'Detected OS: macOS — there is currently no macOS build of Ezoo Download; releases are for Windows and Linux only.',
    'os.other': 'The detected OS does not match a supported platform — Ezoo Download is available for Windows and Linux only.',
    'dl.detected': '✓ Your system',
    'dl.win.t': '🪟 Windows 10 / 11', 'dl.win.d': '— per-user setup, no administrator rights needed.', 'dl.win.b': 'Download Setup.exe',
    'dl.deb.t': '🐧 Linux — Debian / Ubuntu', 'dl.deb.d': '— with bundled ffmpeg.', 'dl.deb.b': 'Download .deb',
    'dl.app.t': '🐧 Linux — any distro', 'dl.app.d': '— no install needed, with bundled ffmpeg.', 'dl.app.b': 'Download AppImage',
    'dl.rpm.t': '🐧 Linux — Fedora / RHEL', 'dl.rpm.d': '— with bundled ffmpeg.', 'dl.rpm.b': 'Download .rpm',
    'dl.flat.t': '📦 Linux — Flatpak', 'dl.flat.d': '— a single Flatpak bundle (app ID io.github.kerlif.EzooDownload).', 'dl.flat.b': 'Download Flatpak bundle',
    'dl.crx.t': '🌐 Chrome extension', 'dl.crx.d': '— signed with the original key, same extension ID.', 'dl.crx.b': 'Download .crx', 'dl.zip.b': 'Download .zip (manual load)',
    'dl.xpi.t': '🦊 Firefox extension', 'dl.xpi.b': 'Download .xpi',
    'dl.checks': 'Verify file integrity with <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS.txt">SHA256SUMS.txt</a> (Linux) or <a href="https://github.com/kerlif/ezoo-download/releases/latest/download/SHA256SUMS-windows.txt">SHA256SUMS-windows.txt</a> (Windows) · <a href="https://github.com/kerlif/ezoo-download/releases">All releases & changelog</a>.',
    'install.title': 'Install guide',
    'install.sub': 'Exact commands for every platform — click “Copy”, then paste into the terminal.',
    'g.win.t': 'Windows 10 / 11 (64-bit)',
    'g.win.1': 'Download <code>EzooDownload-Setup.exe</code> from the download section above.',
    'g.win.2': 'Run the file — it installs for the current user and needs no administrator rights.',
    'g.win.3': 'After installing, open the app from the Start menu. SmartScreen may warn because the app is not signed with a code-signing certificate.',
    'g.deb.t': 'Linux — .deb (Debian / Ubuntu / Mint)',
    'g.deb.1': 'Download the <code>.deb</code> file, then install it with the command below.',
    'g.deb.2': 'If a missing-dependencies message appears, resolve it with the second command.',
    'g.rpm.t': 'Linux — .rpm (Fedora / RHEL / openSUSE)',
    'g.rpm.1': 'Download the <code>.rpm</code> file, then install it with either command below.',
    'g.app.t': 'Linux — AppImage (any distro)',
    'g.app.1': 'Make the file executable, then run it — no installation needed.',
    'g.app.2': 'If it does not run directly, your system is probably missing FUSE: install <code>libfuse2</code>, or extract the file and run it from the resulting folder.',
    'g.flat.t': 'Linux — Flatpak',
    'g.flat.1': 'Download the bundle file <code>EzooDownload-x86_64.flatpak</code> — this bundle is not published on Flathub; install it straight from the bundle file.',
    'g.flat.2': 'Install the bundle with the command below, then run the app with the second command.',
    'g.ext.t': 'Browser extension (Chrome / Firefox)',
    'g.ext.1': 'Chrome (and Edge, Brave, Opera): turn on <strong>Developer mode</strong> on <code>chrome://extensions</code>, then drag the <code>.crx</code> file onto the page. If Chrome warns about an extension not published in the store, choose <strong>Keep</strong>.',
    'g.ext.2': 'Chrome alternative: load the unpacked <code>.zip</code> from <code>chrome://extensions</code> with <strong>Load unpacked</strong>.',
    'g.ext.3': 'Firefox: try the extension temporarily from <code>about:debugging#/runtime/this-firefox</code> → <strong>Load Temporary Add-on</strong> → pick the <code>.xpi</code> file (it lasts until Firefox closes). Permanent installation requires Mozilla signing; it can be used permanently on Firefox Developer Edition / Nightly after disabling <code>xpinstall.signatures.required</code> in <code>about:config</code>.',
    'g.ext.4': 'After installing the extension, open the app and links will reach it automatically over the local bridge.',
    'g.ext.note': 'This is the Chrome extension’s fixed ID — the same key signs every release.',
    'copy.btn': 'Copy',
    'shots.title': 'Take a look',
    'shots.note': 'Screenshots of the 1.1.0 interface: the main window, download progress, and the add window with stream duration, quality, and estimated size.',
    'shots.1.alt': 'The Ezoo Download main window in Arabic: the empty download list and categories on the side',
    'shots.1.cap': 'Main window',
    'shots.2.alt': 'The download progress window in Ezoo Download in Arabic, with the progress bar and speed',
    'shots.2.cap': 'Download progress',
    'shots.3.alt': 'The add-new-download window in Ezoo Download in Arabic: an HLS stream with duration, quality, and estimated size',
    'shots.3.cap': 'Adding a new download',
    'faq.title': 'Frequently asked questions',
    'faq.1q': 'Is the app really free?',
    'faq.1a': 'Yes — completely free, with no ads and no features locked behind a subscription.',
    'faq.2q': 'Is the source code open?',
    'faq.2a': 'Yes. The app is written in Rust, licensed MIT, and the full source is in the <a href="https://github.com/kerlif/ezoo-download">repository</a>.',
    'faq.3q': 'Which systems does it run on?',
    'faq.3a': 'Windows 10 / 11 (64-bit) and Linux x86_64: .deb, .rpm, AppImage, and Flatpak packages. There is no macOS build.',
    'faq.4q': 'What if the connection drops mid-download?',
    'faq.4a': 'The app resumes automatically from where it stopped, and you can pause and resume manually at any time — even after closing and reopening the app.',
    'faq.5q': 'Does the app collect my data?',
    'faq.5a': 'No. It collects no usage data and sends no statistics anywhere. The complete list of outbound requests is in the <a href="#privacy">Privacy section</a> below.',
    'faq.6q': 'How do updates work?',
    'faq.6a': 'The app checks this repository’s releases at startup and every 24 hours (the check can be turned off in Settings), and only offers an update after verifying the signed version.json manifest and the file’s hash. The .deb, .rpm, and Flatpak copies only notify — they do not update themselves.',
    'faq.7q': 'Does picking a quality in the capture window download that quality?',
    'faq.7a': 'Not yet: the selector shows every available quality with an estimated size for each, but downloads currently take the best quality. Steering a download to the selected quality is planned work.',
    'faq.8q': 'Why does the app refuse some video links?',
    'faq.8a': 'Streams protected by DRM systems (such as Widevine, PlayReady, and FairPlay) are refused on purpose: the app blocks them and never tries to circumvent them.',
    'privacy.title': 'Privacy: what does the app actually contact?',
    'privacy.intro': 'No telemetry, no analytics, no crash reports anywhere in the app. This is the complete list of outbound requests — nothing outside it:',
    'privacy.1': '<strong>Update checker</strong> (automatic, at startup and every 24 hours, can be turned off): one request to <code>api.github.com</code> to learn the latest release, then the signed <code>version.json</code> manifest and the checksums file. The only header sent is the app name and version; no user or machine data is sent.',
    'privacy.2': '<strong>Tools updater</strong> (automatic, at most once a day): fetches the <code>yt-dlp</code> build and the <code>aria2</code> file from their GitHub releases and hash-checks them before use. ffmpeg is never fetched — it is bundled in the package.',
    'privacy.3': '<strong>Downloads themselves</strong> (on your action): they go to the server of the link you added only, with <code>Range</code> requests for splitting and resuming.',
    'privacy.4': '<strong>Link probing in the capture window</strong> (on your action, about 5 seconds per stage): a <code>HEAD</code> request and a one-byte <code>Range</code> request, plus reading <code>.m3u8</code> / <code>.mpd</code> playlists — all to the link’s own server only; no third-party metadata or thumbnail service is queried.',
    'privacy.5': '<strong>Per-host learning</strong>: the best connection count and mean segment size per host, stored locally only in the settings file (host name only, no URLs or file names, pruned after 30 days). Nothing of it is sent anywhere.',
    'privacy.6': '<strong>Extension bridge</strong>: inbound only, on <code>127.0.0.1:9614</code> on your machine, with no outbound network traffic.',
    'privacy.7': '<strong>yt-dlp</strong>: runs only when you ask for video choices, playlist lookups, or its update from the Help menu — it then contacts the video sites themselves, which is yt-dlp’s behaviour, not the app’s.',
    'privacy.link': 'The full, detailed list is in <a href="https://github.com/kerlif/ezoo-download/blob/main/docs/PRIVACY.md">docs/PRIVACY.md</a>.',
    'license.title': 'License & credits',
    'license.body': 'The app is licensed MIT, © 2026 <strong>Ahmed Ezzat</strong> (Ezoo Tech) — the original project is <a href="https://github.com/AhmedEzo686/ezoo-download">AhmedEzo686/ezoo-download</a>. This is a separate fork at <a href="https://github.com/kerlif/ezoo-download">kerlif/ezoo-download</a> that added Linux support, adaptive connections, the update checker, and the Flatpak package, with all rights and credit kept for the original author.',
    'license.fonts': 'This site’s fonts: Carlito and Noto Sans Arabic (SIL Open Font License 1.1) — the license texts are in <a href="assets/fonts/OFL-Carlito.txt">OFL-Carlito.txt</a> and <a href="assets/fonts/OFL-NotoSansArabic.txt">OFL-NotoSansArabic.txt</a>. The site uses no frameworks, no trackers, and no third-party scripts.',
    'license.changelog': 'Changelog — releases page',
    'footer.credits': 'MIT © 2026 Ahmed Ezzat (Ezoo Tech) · Separate fork & Linux support: kerlif · Original project: <a href="https://github.com/AhmedEzo686/ezoo-download">AhmedEzo686/ezoo-download</a>',
    'footer.releases': 'Releases', 'footer.issues': 'Report an issue', 'footer.license': 'License',
    'copy.copied': '✓ Copied', 'copy.manual': 'Copy manually'
  }
};

let currentLang = 'ar';
let currentTheme = 'light';

function storeGet(key) {
  try { return window.localStorage.getItem(key); } catch (_) { return null; }
}
function storeSet(key, value) {
  try { window.localStorage.setItem(key, value); } catch (_) { /* private mode: ignore */ }
}

function applyLang(lang) {
  if (!I18N[lang]) lang = 'ar';
  currentLang = lang;
  const dict = I18N[lang];
  document.documentElement.lang = lang;
  document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
  document.title = dict['meta.title'];
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n');
    if (dict[key] !== undefined) el.textContent = dict[key];
  });
  document.querySelectorAll('[data-i18n-html]').forEach((el) => {
    const key = el.getAttribute('data-i18n-html');
    if (dict[key] !== undefined) el.innerHTML = dict[key];
  });
  document.querySelectorAll('[data-i18n-alt]').forEach((el) => {
    const key = el.getAttribute('data-i18n-alt');
    if (dict[key] !== undefined) el.setAttribute('alt', dict[key]);
  });
  document.querySelectorAll('[data-i18n-aria]').forEach((el) => {
    const key = el.getAttribute('data-i18n-aria');
    if (dict[key] !== undefined) el.setAttribute('aria-label', dict[key]);
  });
  document.querySelectorAll('[data-i18n-content]').forEach((el) => {
    const key = el.getAttribute('data-i18n-content');
    if (dict[key] !== undefined) el.setAttribute('content', dict[key]);
  });
  document.getElementById('lang-ar').setAttribute('aria-pressed', String(lang === 'ar'));
  document.getElementById('lang-en').setAttribute('aria-pressed', String(lang === 'en'));
  updateThemeToggle();
  if (typeof refreshSimulatorText === 'function') refreshSimulatorText();
}

function updateThemeToggle() {
  const btn = document.getElementById('theme-toggle');
  if (!btn) return;
  const dict = I18N[currentLang];
  const icon = btn.querySelector('span[aria-hidden]');
  const label = btn.querySelector('span[data-i18n]');
  btn.setAttribute('aria-pressed', String(currentTheme === 'dark'));
  if (icon) icon.textContent = currentTheme === 'dark' ? '☀' : '☾';
  if (label) label.textContent = currentTheme === 'dark' ? dict['theme.light'] : dict['theme.dark'];
}

function applyTheme(theme) {
  currentTheme = theme === 'dark' ? 'dark' : 'light';
  document.documentElement.setAttribute('data-theme', currentTheme);
  updateThemeToggle();
  if (typeof drawChartExternal === 'function') drawChartExternal();
}

document.getElementById('lang-ar').addEventListener('click', () => { storeSet('ezoo-site-lang', 'ar'); applyLang('ar'); });
document.getElementById('lang-en').addEventListener('click', () => { storeSet('ezoo-site-lang', 'en'); applyLang('en'); });
document.getElementById('theme-toggle').addEventListener('click', () => {
  const next = currentTheme === 'dark' ? 'light' : 'dark';
  storeSet('ezoo-site-theme', next);
  applyTheme(next);
});

/* Init: stored choices win; theme falls back to prefers-color-scheme. */
(function initPrefs() {
  const storedTheme = storeGet('ezoo-site-theme');
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  applyTheme(storedTheme || (prefersDark ? 'dark' : 'light'));
  applyLang(storeGet('ezoo-site-lang') || 'ar');
})();

/* ------------------------------------------------------- OS detection */
(function detectOS() {
  const ua = (navigator.userAgent || '').toLowerCase();
  const platform = ((navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '').toLowerCase();
  const note = document.getElementById('os-note');
  const cards = document.getElementById('dl-cards');
  if (!note || !cards) return;
  const dict = () => I18N[currentLang];
  let os = 'unknown';
  if (/android/.test(ua)) os = 'android';
  else if (/iphone|ipad|ipod/.test(ua) || /ios/.test(platform)) os = 'ios';
  else if (/win/.test(platform) || /windows/.test(ua)) os = 'windows';
  else if (/mac/.test(platform) || /mac os/.test(ua)) os = 'macos';
  else if (/linux/.test(platform) || /linux/.test(ua)) os = 'linux';
  const target = os === 'windows' ? 'windows' : os === 'linux' ? 'appimage' : null;
  if (target) {
    const card = cards.querySelector('[data-os="' + target + '"]');
    if (card) {
      cards.insertBefore(card, cards.firstChild);
      card.classList.add('detected');
      const badge = card.querySelector('.detected-badge');
      if (badge) badge.hidden = false;
    }
    note.textContent = dict()[os === 'windows' ? 'os.windows' : 'os.linux'];
  } else if (os === 'macos') {
    note.textContent = dict()['os.macos'];
  } else if (os === 'android' || os === 'ios') {
    note.textContent = dict()['os.other'];
  } else {
    note.textContent = dict()['os.unknown'];
  }
  /* Keep the note in sync when the language changes. */
  document.getElementById('lang-ar').addEventListener('click', () => { note.textContent = I18N.ar[target ? (os === 'windows' ? 'os.windows' : 'os.linux') : os === 'macos' ? 'os.macos' : (os === 'android' || os === 'ios') ? 'os.other' : 'os.unknown']; });
  document.getElementById('lang-en').addEventListener('click', () => { note.textContent = I18N.en[target ? (os === 'windows' ? 'os.windows' : 'os.linux') : os === 'macos' ? 'os.macos' : (os === 'android' || os === 'ios') ? 'os.other' : 'os.unknown']; });
})();

const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)').matches;

/* Ripple on buttons */
document.addEventListener('click', (e) => {
  const btn = e.target.closest('.btn, .copy-btn');
  if (!btn || reducedMotion) return;
  const rect = btn.getBoundingClientRect();
  const ripple = document.createElement('span');
  ripple.className = 'ripple';
  const size = Math.max(rect.width, rect.height);
  ripple.style.width = ripple.style.height = size + 'px';
  ripple.style.left = (e.clientX - rect.left - size / 2) + 'px';
  ripple.style.top = (e.clientY - rect.top - size / 2) + 'px';
  btn.appendChild(ripple);
  ripple.addEventListener('animationend', () => ripple.remove());
});

/* Click packet burst: little download arrows float up and fade */
if (!reducedMotion) {
  document.addEventListener('pointerdown', (e) => {
    if (e.target.closest('input, select, textarea')) return;
    const colors = ['#8a57f5', '#2e7bf6', '#16a34a'];
    for (let i = 0; i < 7; i++) {
      const p = document.createElement('span');
      p.className = 'packet';
      p.textContent = '↓';
      p.style.left = e.clientX + 'px';
      p.style.top = e.clientY + 'px';
      p.style.color = colors[i % colors.length];
      p.style.fontSize = (14 + Math.random() * 12) + 'px';
      document.body.appendChild(p);
      const dx = (Math.random() - 0.5) * 120;
      const dy = -(50 + Math.random() * 90);
      const rot = (Math.random() - 0.5) * 60;
      p.animate([
        { transform: 'translate(-50%, -50%) rotate(0deg)', opacity: 1 },
        { transform: 'translate(calc(-50% + ' + dx + 'px), calc(-50% + ' + dy + 'px)) rotate(' + rot + 'deg)', opacity: 0 }
      ], { duration: 750 + Math.random() * 350, easing: 'cubic-bezier(.2,.7,.3,1)' }).onfinish = () => p.remove();
    }
  });
}

/* Card tilt (fine pointers only) */
if (finePointer && !reducedMotion) {
  document.querySelectorAll('.card, .dl-card, .shots figure, .guide').forEach((el) => {
    el.addEventListener('pointermove', (e) => {
      const r = el.getBoundingClientRect();
      const rx = ((e.clientY - r.top) / r.height - 0.5) * -5;
      const ry = ((e.clientX - r.left) / r.width - 0.5) * 5;
      el.style.setProperty('--rx', rx.toFixed(2) + 'deg');
      el.style.setProperty('--ry', ry.toFixed(2) + 'deg');
    });
    el.addEventListener('pointerleave', () => {
      el.style.setProperty('--rx', '0deg');
      el.style.setProperty('--ry', '0deg');
    });
  });
}

/* Scroll reveal */
(function () {
  const els = document.querySelectorAll('.card, .step, .dl-card, .shots figure, .demo, .guide, details');
  els.forEach((el) => el.classList.add('reveal'));
  if ('IntersectionObserver' in window && !reducedMotion) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((en) => {
        if (en.isIntersecting) { en.target.classList.add('visible'); io.unobserve(en.target); }
      });
    }, { threshold: 0.12 });
    els.forEach((el) => io.observe(el));
  } else {
    els.forEach((el) => el.classList.add('visible'));
  }
})();

/* Copy buttons */
document.querySelectorAll('.copy-btn').forEach((btn) => {
  btn.addEventListener('click', async () => {
    const text = btn.getAttribute('data-copy') || '';
    let ok = false;
    try { await navigator.clipboard.writeText(text); ok = true; }
    catch (_) {
      const ta = document.createElement('textarea');
      ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
      document.body.appendChild(ta); ta.select();
      try { ok = document.execCommand('copy'); } catch (_) { ok = false; }
      ta.remove();
    }
    const dict = I18N[currentLang];
    btn.textContent = ok ? dict['copy.copied'] : dict['copy.manual'];
    btn.disabled = true;
    setTimeout(() => { btn.textContent = I18N[currentLang]['copy.btn']; btn.disabled = false; }, 1600);
  });
});

/* Download simulator: segmented download with live threads + speed chart.
   Simulation only — no real download happens. */
(function () {
  const panel = document.getElementById('demo-panel');
  if (!panel) return;
  const threadsInput = document.getElementById('threads');
  const threadsValue = document.getElementById('threads-value');
  const sizeSelect = document.getElementById('filesize');
  const startBtn = document.getElementById('sim-start');
  const resetBtn = document.getElementById('sim-reset');
  const statSpeed = document.getElementById('stat-speed');
  const statProgress = document.getElementById('stat-progress');
  const statEta = document.getElementById('stat-eta');
  const statActive = document.getElementById('stat-active');
  const lanesEl = document.getElementById('lanes');
  const resultEl = document.getElementById('demo-result');
  const canvas = document.getElementById('speed-chart');
  const ctx = canvas.getContext('2d');

  const SPEEDS = []; // aggregate speed samples (newest last)
  let lanes = [];
  let running = false;
  let everStarted = false; // manual start OR auto-start: auto-start fires only once
  let timer = null;
  let startedAt = 0;
  let elapsedBefore = 0; // ms accumulated before a pause
  let sizeMB = parseFloat(sizeSelect.value);
  let lastFinish = null; // {secs, avg, single, faster} for re-rendering on language change

  function dict() { return I18N[currentLang]; }

  function buildLanes(progress) {
    const n = parseInt(threadsInput.value, 10);
    lanes = [];
    lanesEl.innerHTML = '';
    for (let i = 0; i < n; i++) {
      const lane = { progress: progress, speed: 0, base: 4 + Math.random() * 6, done: progress >= 1 };
      lane.speed = lane.base * 0.6;
      lanes.push(lane);
      const row = document.createElement('div');
      row.className = 'lane';
      row.innerHTML = '<span class="lane-name">' + dict()['sim.lane'] + ' ' + (i + 1) + '</span>' +
        '<div class="lane-track"><div class="lane-fill"></div></div>' +
        '<span class="lane-speed" dir="ltr">0.0 MB/s</span>';
      lanesEl.appendChild(row);
      lane.fillEl = row.querySelector('.lane-fill');
      lane.speedEl = row.querySelector('.lane-speed');
      renderLane(lane);
    }
  }

  function renderLane(lane) {
    const pct = Math.min(100, lane.progress * 100);
    lane.fillEl.style.width = pct + '%';
    lane.fillEl.classList.toggle('done', lane.done);
    lane.speedEl.textContent = lane.done ? dict()['sim.done'] : lane.speed.toFixed(1) + ' MB/s';
  }

  function totalProgress() {
    if (!lanes.length) return 0;
    return lanes.reduce((s, l) => s + Math.min(1, l.progress), 0) / lanes.length;
  }

  function fmtEta(seconds) {
    if (!isFinite(seconds) || seconds <= 0) return '—';
    const m = Math.floor(seconds / 60), s = Math.round(seconds % 60);
    return m > 0 ? m + ':' + String(s).padStart(2, '0') : s + ' ' + dict()['sim.sec'];
  }

  function cssVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#e7e8f0';
  }

  function drawChart() {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight || 120;
    if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.strokeStyle = cssVar('--line'); ctx.lineWidth = 1;
    for (let g = 1; g <= 3; g++) {
      const y = (h / 4) * g;
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }
    if (SPEEDS.length < 2) return;
    const max = Math.max.apply(null, SPEEDS.concat([10]));
    const grad = ctx.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, 'rgba(138,87,245,.35)');
    grad.addColorStop(1, 'rgba(46,123,246,.02)');
    const stepX = w / 59;
    ctx.beginPath();
    SPEEDS.forEach((v, i) => {
      const x = w - (SPEEDS.length - 1 - i) * stepX;
      const y = h - (v / max) * (h - 14) - 4;
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = cssVar('--violet'); ctx.lineWidth = 2; ctx.stroke();
    ctx.lineTo(w, h);
    ctx.lineTo(w - (SPEEDS.length - 1) * stepX, h);
    ctx.closePath();
    ctx.fillStyle = grad; ctx.fill();
  }
  drawChartExternal = drawChart;

  function tick() {
    const dt = 0.1; // seconds per tick
    let active = 0, agg = 0;
    lanes.forEach((lane) => {
      if (lane.done) { lane.speed = 0; return; }
      active++;
      lane.speed = Math.max(0.5, lane.speed + (Math.random() - 0.5) * 2.4);
      lane.speed += (lane.base - lane.speed) * 0.08;
      lane.progress += (lane.speed * dt) / (sizeMB / lanes.length);
      if (lane.progress >= 1) { lane.progress = 1; lane.done = true; lane.speed = 0; }
      agg += lane.speed;
      renderLane(lane);
    });
    const p = totalProgress();
    SPEEDS.push(agg);
    if (SPEEDS.length > 60) SPEEDS.shift();
    statSpeed.textContent = agg.toFixed(1);
    statProgress.textContent = Math.floor(p * 100) + '%';
    statActive.textContent = active;
    statEta.textContent = agg > 0.5 ? fmtEta((sizeMB * (1 - p)) / agg) : '—';
    drawChart();
    if (p >= 1) finish();
  }

  function elapsedMs() {
    return elapsedBefore + (running ? performance.now() - startedAt : 0);
  }

  function stopTimer() {
    if (timer) { clearInterval(timer); timer = null; }
    if (running) { elapsedBefore += performance.now() - startedAt; }
  }

  function renderResult() {
    if (!lastFinish) return;
    resultEl.textContent = dict()['sim.result']
      .replace('{secs}', lastFinish.secs).replace('{avg}', lastFinish.avg)
      .replace('{single}', lastFinish.single).replace('{faster}', lastFinish.faster);
  }

  function finish() {
    stopTimer();
    running = false;
    startBtn.textContent = dict()['sim.start'];
    startBtn.disabled = true;
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
    const secs = elapsedMs() / 1000;
    const avg = sizeMB / Math.max(secs, 0.1);
    // Single-connection estimate under the same simulated conditions:
    // the average of the lanes' base speeds, as if only one ran.
    const singleBase = lanes.length ? lanes.reduce((s, l) => s + l.base, 0) / lanes.length : 6;
    const singleSecs = sizeMB / singleBase;
    const faster = Math.max(1, Math.round(singleSecs / Math.max(secs, 0.1)));
    lastFinish = { secs: secs.toFixed(1), avg: avg.toFixed(1), single: Math.round(singleSecs), faster: faster };
    resultEl.hidden = false;
    renderResult();
  }

  function start() {
    if (totalProgress() >= 1) reset(false);
    everStarted = true;
    running = true;
    startedAt = performance.now();
    threadsInput.disabled = true;
    sizeSelect.disabled = true;
    startBtn.textContent = dict()['sim.pause'];
    resultEl.hidden = true;
    timer = setInterval(tick, 100);
  }

  function pause() {
    stopTimer();
    running = false;
    startBtn.textContent = dict()['sim.resume'];
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
  }

  function reset(clearResult) {
    stopTimer();
    running = false;
    everStarted = true; // a reset is a manual interaction: never auto-start after it
    elapsedBefore = 0;
    SPEEDS.length = 0;
    lastFinish = null;
    sizeMB = parseFloat(sizeSelect.value);
    buildLanes(0);
    statSpeed.textContent = '0.0';
    statProgress.textContent = '0%';
    statEta.textContent = '—';
    statActive.textContent = '0';
    startBtn.textContent = dict()['sim.start'];
    startBtn.disabled = false;
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
    if (clearResult !== false) resultEl.hidden = true;
    drawChart();
  }

  /* Re-render language-dependent simulator text without touching progress. */
  refreshSimulatorText = function () {
    const p = totalProgress();
    const saved = lanes.map((l) => ({ progress: l.progress, speed: l.speed, base: l.base, done: l.done }));
    buildLanes(p);
    lanes.forEach((l, i) => { l.progress = saved[i].progress; l.speed = saved[i].speed; l.base = saved[i].base; l.done = saved[i].done; renderLane(l); });
    startBtn.textContent = running ? dict()['sim.pause'] : (p >= 1 ? dict()['sim.start'] : (elapsedBefore > 0 ? dict()['sim.resume'] : dict()['sim.start']));
    statEta.textContent = statEta.textContent === '—' ? '—' : statEta.textContent;
    if (!resultEl.hidden) renderResult();
    drawChart();
  };

  startBtn.addEventListener('click', () => { if (running) pause(); else start(); });
  resetBtn.addEventListener('click', () => reset(true));
  threadsInput.addEventListener('input', () => {
    threadsValue.textContent = threadsInput.value;
    if (!running && totalProgress() < 1) buildLanes(totalProgress());
  });
  threadsInput.addEventListener('change', () => {
    if (running) return;
    if (totalProgress() >= 1) reset(true); else buildLanes(totalProgress());
  });
  sizeSelect.addEventListener('change', () => reset(true));
  window.addEventListener('resize', drawChart);

  threadsValue.textContent = threadsInput.value;
  buildLanes(0);
  drawChart();

  // Auto-start once when the panel scrolls into view (defaults: 8 threads,
  // 500 MB). Never re-fires: not after a manual start, pause, or reset,
  // and never when the user prefers reduced motion.
  if (!reducedMotion && 'IntersectionObserver' in window) {
    const autoIO = new IntersectionObserver((entries) => {
      entries.forEach((en) => {
        if (everStarted) { autoIO.disconnect(); return; }
        if (en.isIntersecting && !running && totalProgress() === 0 && document.visibilityState === 'visible') {
          autoIO.disconnect();
          start();
        }
      });
    }, { threshold: 0.4 });
    autoIO.observe(panel);
  }
})();
