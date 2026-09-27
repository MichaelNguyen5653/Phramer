// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/welcometour.h"

#include "core/flameshot.h"
#include "utils/colorutils.h"
#include "utils/confighandler.h"
#include "utils/globalvalues.h"
#include "utils/pathinfo.h"
#if defined(Q_OS_WIN)
#include "utils/screenclipprotocol.h"
#endif
#include "widgets/editor/editortheme.h"
#include "widgets/editor/editorwindow.h"

#include <QCheckBox>
#include <QEasingCurve>
#include <QGraphicsOpacityEffect>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QKeyEvent>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QParallelAnimationGroup>
#include <QPointer>
#include <QPushButton>
#include <QSequentialAnimationGroup>
#include <QStackedLayout>
#include <QTimer>
#include <QVBoxLayout>
#include <QVariantAnimation>
#include <QVersionNumber>

#include <cmath>

namespace {

constexpr int CardWidth = 760;
constexpr int CardHeight = 520;
// Room around the card for its shadow; the window itself is transparent
constexpr int ShadowMargin = 28;
constexpr int CardRadius = 18;
constexpr int RevealMs = 420;
constexpr int RevealStaggerMs = 70;
constexpr int FadeOutMs = 140;

QColor brandBlue()
{
    return { 56, 153, 194 };
}

QString rgba(const QColor& c)
{
    return QStringLiteral("rgba(%1,%2,%3,%4)")
      .arg(c.red())
      .arg(c.green())
      .arg(c.blue())
      .arg(c.alpha());
}

int majorOf(const QString& version)
{
    QString text = version.trimmed();
    if (text.startsWith(QLatin1Char('v'))) {
        text.remove(0, 1);
    }
    const QVersionNumber parsed = QVersionNumber::fromString(text);
    return parsed.isNull() ? -1 : parsed.majorVersion();
}

} // namespace

/**
 * @brief The app icon, scaling in over a softly breathing glow.
 *
 * Driven by two QVariantAnimations rather than Q_PROPERTYs, so it needs no
 * moc of its own.
 */
class WelcomeLogo : public QWidget
{
public:
    explicit WelcomeLogo(QWidget* parent)
      : QWidget(parent)
      , m_icon(QStringLiteral(":img/app/appicon-512.png"))
    {
        setFixedSize(190, 190);

        m_intro = new QVariantAnimation(this);
        m_intro->setStartValue(0.0);
        m_intro->setEndValue(1.0);
        m_intro->setDuration(900);
        connect(m_intro,
                &QVariantAnimation::valueChanged,
                this,
                [this](const QVariant& v) {
                    m_progress = v.toReal();
                    update();
                });

        m_breath = new QVariantAnimation(this);
        m_breath->setStartValue(0.0);
        m_breath->setKeyValueAt(0.5, 1.0);
        m_breath->setEndValue(0.0);
        m_breath->setDuration(2600);
        m_breath->setEasingCurve(QEasingCurve::InOutSine);
        m_breath->setLoopCount(-1);
        connect(m_breath,
                &QVariantAnimation::valueChanged,
                this,
                [this](const QVariant& v) {
                    m_glow = v.toReal();
                    update();
                });
    }

    void play()
    {
        m_progress = 0.0;
        m_intro->start();
        m_breath->start();
    }

    // The glow loops forever, so it must stop whenever the page is left
    void stop() { m_breath->stop(); }

protected:
    void paintEvent(QPaintEvent*) override
    {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setRenderHint(QPainter::SmoothPixmapTransform);

        const QPointF centre = QRectF(rect()).center();
        const qreal appear =
          QEasingCurve(QEasingCurve::OutCubic).valueForProgress(m_progress);
        // Overshoots a touch past full size and settles, like a pop
        const qreal scale =
          0.62 +
          0.38 *
            QEasingCurve(QEasingCurve::OutBack).valueForProgress(m_progress);

        // Glow first, under the icon
        QColor glow = brandBlue();
        glow.setAlphaF(appear * (0.22 + 0.18 * m_glow));
        QRadialGradient gradient(centre, width() / 2.0);
        gradient.setColorAt(0.0, glow);
        glow.setAlphaF(0.0);
        gradient.setColorAt(1.0, glow);
        painter.setPen(Qt::NoPen);
        painter.setBrush(gradient);
        painter.drawEllipse(rect());

        if (m_icon.isNull()) {
            return;
        }
        const qreal side = 112 * scale;
        painter.setOpacity(appear);
        painter.drawPixmap(
          QRectF(centre.x() - side / 2, centre.y() - side / 2, side, side),
          m_icon,
          QRectF(m_icon.rect()));
    }

private:
    QPixmap m_icon;
    QVariantAnimation* m_intro{ nullptr };
    QVariantAnimation* m_breath{ nullptr };
    qreal m_progress{ 0.0 };
    qreal m_glow{ 0.0 };
};

/**
 * @brief A keyboard photo that zooms in on the Print Screen key, then rings
 * it with a pulsing outline.
 *
 * Plays like a short clip each time its page is entered. The picture always
 * covers the frame: the pan is clamped so no edge of it ever shows.
 */
class PrintScreenZoom : public QWidget
{
public:
    explicit PrintScreenZoom(QWidget* parent)
      : QWidget(parent)
      , m_image(QStringLiteral(":img/app/printscreen-key.png"))
    {
        setFixedHeight(196);

        m_zoom = new QVariantAnimation(this);
        m_zoom->setStartValue(0.0);
        m_zoom->setEndValue(1.0);
        m_zoom->setDuration(2000);
        m_zoom->setEasingCurve(QEasingCurve::InOutCubic);
        connect(m_zoom,
                &QVariantAnimation::valueChanged,
                this,
                [this](const QVariant& v) {
                    m_progress = v.toReal();
                    update();
                });
        connect(m_zoom, &QVariantAnimation::finished, this, [this]() {
            m_pulse->start();
        });

        m_pulse = new QVariantAnimation(this);
        m_pulse->setStartValue(0.0);
        m_pulse->setKeyValueAt(0.5, 1.0);
        m_pulse->setEndValue(0.0);
        m_pulse->setDuration(1400);
        m_pulse->setEasingCurve(QEasingCurve::InOutSine);
        m_pulse->setLoopCount(-1);
        connect(m_pulse,
                &QVariantAnimation::valueChanged,
                this,
                [this](const QVariant& v) {
                    m_glow = v.toReal();
                    update();
                });

        // Holds the wide shot a moment before moving, like a clip's first
        // frame
        m_delay = new QTimer(this);
        m_delay->setSingleShot(true);
        m_delay->setInterval(450);
        connect(m_delay, &QTimer::timeout, this, [this]() { m_zoom->start(); });
    }

    void play()
    {
        stop();
        m_progress = 0.0;
        m_glow = 0.0;
        update();
        m_delay->start();
    }

    // The pulse loops forever, so it must stop whenever the page is left
    void stop()
    {
        m_delay->stop();
        m_zoom->stop();
        m_pulse->stop();
    }

protected:
    void paintEvent(QPaintEvent*) override
    {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setRenderHint(QPainter::SmoothPixmapTransform);

        const QRectF frame = rect();
        QPainterPath clip;
        clip.addRoundedRect(frame, 12, 12);
        painter.setClipPath(clip);
        painter.fillRect(frame, QColor(62, 62, 62));
        if (m_image.isNull()) {
            return;
        }

        // Where the key sits in the photo, in its own pixels
        const QRectF key(180, 29, 46, 48);
        const QSizeF image = m_image.size();

        // Starts on the whole keyboard, just covering the frame, and ends
        // with the key filling most of its height. Scale is interpolated
        // geometrically so the zoom seems to move at an even speed.
        const qreal from =
          qMax(frame.width() / image.width(), frame.height() / image.height());
        const qreal to = frame.height() * 0.62 / key.height();
        const qreal scale = from * std::pow(to / from, m_progress);

        // Pans from the keyboard's middle to just below the key, so the
        // arrow pointing at it stays in the shot
        const QPointF start(image.width() / 2, image.height() * 0.42);
        const QPointF end = key.center() + QPointF(0, 12);
        const QPointF focus = start + (end - start) * m_progress;

        const qreal x = qBound(frame.width() - image.width() * scale,
                               frame.width() / 2 - focus.x() * scale,
                               0.0);
        const qreal y = qBound(frame.height() - image.height() * scale,
                               frame.height() / 2 - focus.y() * scale,
                               0.0);
        painter.drawPixmap(
          QRectF(x, y, image.width() * scale, image.height() * scale),
          m_image,
          QRectF(QPointF(0, 0), image));

        // A soft vignette, which is most of what makes it read as footage
        QRadialGradient vignette(frame.center(), frame.width() * 0.62);
        vignette.setColorAt(0.55, QColor(0, 0, 0, 0));
        vignette.setColorAt(1.0, QColor(0, 0, 0, 110));
        painter.fillRect(frame, vignette);

        // The ring arrives over the last stretch of the zoom, then breathes
        const qreal arrive = qBound(0.0, (m_progress - 0.8) / 0.2, 1.0);
        if (arrive > 0.0) {
            const QRectF ring(x + key.left() * scale,
                              y + key.top() * scale,
                              key.width() * scale,
                              key.height() * scale);
            const qreal grow = 4 + 5 * m_glow;
            QColor colour = brandBlue();
            colour.setAlphaF(arrive * (0.55 + 0.45 * m_glow));
            painter.setPen(QPen(colour, 3));
            painter.setBrush(Qt::NoBrush);
            painter.drawRoundedRect(
              ring.adjusted(-grow, -grow, grow, grow), 12, 12);
        }
    }

private:
    QPixmap m_image;
    QVariantAnimation* m_zoom{ nullptr };
    QVariantAnimation* m_pulse{ nullptr };
    QTimer* m_delay{ nullptr };
    qreal m_progress{ 0.0 };
    qreal m_glow{ 0.0 };
};

WelcomeTour::WelcomeTour(QWidget* parent)
  : QDialog(parent)
{
    setWindowTitle(tr("Welcome to Phramer"));
    setWindowIcon(GlobalValues::appIcon());
    setModal(true);
    // Frameless and transparent: the card and its shadow are painted here
    setWindowFlags(Qt::Dialog | Qt::FramelessWindowHint);
    setAttribute(Qt::WA_TranslucentBackground);
    setFixedSize(CardWidth + 2 * ShadowMargin, CardHeight + 2 * ShadowMargin);

    const EditorTheme theme = EditorTheme::current();
    m_dark = theme.dark;
    setPalette(theme.palette(palette()));

    setStyleSheet(
      QStringLiteral(
        "QLabel { color: %1; background: transparent; }"
        "QLabel#muted { color: %2; }"
        "QPushButton { color: %1; background: transparent;"
        "  border: 1px solid %3; border-radius: 9px; padding: 8px 18px; }"
        "QPushButton:hover { background: %4; }"
        "QPushButton:pressed { background: %5; }"
        "QPushButton#primary { color: white; background: %6; border: none; }"
        "QPushButton#primary:hover { background: %7; }"
        "QPushButton#ghost { border: none; color: %2; }"
        "QPushButton#ghost:hover { color: %1; background: %4; }"
        "QCheckBox { color: %1; spacing: 10px; background: transparent; }"
        "QCheckBox:disabled { color: %2; }"
        // The native indicator is drawn in the system palette and all but
        // vanishes on the dark card, so it is drawn here in full
        "QCheckBox::indicator { width: 16px; height: 16px;"
        "  border: 2px solid %2; border-radius: 5px; background: %8; }"
        "QCheckBox::indicator:hover { border-color: %6; }"
        "QCheckBox::indicator:checked { border-color: %6; background: %6;"
        "  image: url(:/img/material/white/accept.svg); }"
        "QCheckBox::indicator:checked:disabled { border-color: %2;"
        "  background: %2; }")
        .arg(rgba(theme.text),
             rgba(theme.muted),
             rgba(theme.border),
             rgba(theme.hover),
             rgba(theme.pressed),
             brandBlue().name(),
             brandBlue().lighter(112).name(),
             rgba(theme.surface)));

    m_card = new QWidget(this);
    m_card->setGeometry(ShadowMargin, ShadowMargin, CardWidth, CardHeight);
    auto* cardLayout = new QVBoxLayout(m_card);
    cardLayout->setContentsMargins(40, 36, 40, 28);
    cardLayout->setSpacing(0);

    m_content = new QWidget(m_card);
    auto* stack = new QStackedLayout(m_content);
    stack->setContentsMargins(0, 0, 0, 0);
    cardLayout->addWidget(m_content, 1);

    buildWelcomePage();
#if defined(Q_OS_WIN)
    buildScreenClipPage();
#endif
    buildFeaturesPage();
    buildFixesPage();
    for (const Page& page : m_pages) {
        stack->addWidget(page.widget);
        for (QWidget* item : page.reveal) {
            auto* effect = new QGraphicsOpacityEffect(item);
            effect->setOpacity(0.0);
            item->setGraphicsEffect(effect);
        }
    }

    buildFooter();

    m_revealAnimation = new QParallelAnimationGroup(this);
}

QWidget* WelcomeTour::makeTitle(const QString& text,
                                QWidget* parent,
                                qreal scale)
{
    auto* label = new QLabel(text, parent);
    QFont font = label->font();
    font.setPointSizeF(font.pointSizeF() * scale);
    font.setWeight(QFont::Bold);
    label->setFont(font);
    label->setWordWrap(true);
    return label;
}

QWidget* WelcomeTour::makeRow(const QString& icon,
                              const QString& title,
                              const QString& body,
                              QWidget* parent)
{
    auto* row = new QWidget(parent);
    auto* layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 6, 0, 6);
    layout->setSpacing(14);

    // The icon sits on a tinted tile, the way Raycast lists its commands
    auto* tile = new QLabel(row);
    tile->setFixedSize(36, 36);
    tile->setAlignment(Qt::AlignCenter);
    QColor tint = brandBlue();
    tint.setAlpha(m_dark ? 60 : 36);
    tile->setStyleSheet(
      QStringLiteral("QLabel { background: %1; border-radius: 9px; }")
        .arg(rgba(tint)));
    if (!icon.isEmpty()) {
        const QString dir =
          m_dark ? PathInfo::whiteIconPath() : PathInfo::blackIconPath();
        tile->setPixmap(QIcon(dir + icon).pixmap(20, 20));
    }
    layout->addWidget(tile, 0, Qt::AlignTop);

    auto* text = new QVBoxLayout;
    text->setSpacing(2);
    auto* heading = new QLabel(title, row);
    heading->setWordWrap(true);
    QFont bold = heading->font();
    bold.setWeight(QFont::DemiBold);
    heading->setFont(bold);
    text->addWidget(heading);
    if (!body.isEmpty()) {
        auto* detail = new QLabel(body, row);
        detail->setObjectName(QStringLiteral("muted"));
        detail->setWordWrap(true);
        text->addWidget(detail);
    }
    layout->addLayout(text, 1);
    return row;
}

void WelcomeTour::buildWelcomePage()
{
    Page page;
    page.widget = new QWidget(m_content);
    auto* layout = new QVBoxLayout(page.widget);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(10);
    layout->addStretch(1);

    m_logo = new WelcomeLogo(page.widget);
    layout->addWidget(m_logo, 0, Qt::AlignHCenter);

    auto* title = static_cast<QLabel*>(
      makeTitle(tr("Welcome to the new Phramer"), page.widget, 2.1));
    title->setAlignment(Qt::AlignHCenter);
    layout->addWidget(title);

    auto* subtitle = new QLabel(
      tr("A cleaner look, a smoother editor, and everything right where you "
         "expect it."),
      page.widget);
    subtitle->setObjectName(QStringLiteral("muted"));
    subtitle->setAlignment(Qt::AlignHCenter);
    subtitle->setWordWrap(true);
    layout->addWidget(subtitle);

    auto* hint = new QLabel(tr("Press Enter to continue"), page.widget);
    hint->setObjectName(QStringLiteral("muted"));
    hint->setAlignment(Qt::AlignHCenter);
    QFont small = hint->font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    hint->setFont(small);
    layout->addSpacing(8);
    layout->addWidget(hint);
    layout->addStretch(1);

    // The logo animates itself; only the text is revealed
    page.reveal = { title, subtitle, hint };
    m_pages.append(page);
}

#if defined(Q_OS_WIN)
void WelcomeTour::buildScreenClipPage()
{
    Page page;
    page.widget = new QWidget(m_content);
    m_screenClipPage = page.widget;
    auto* layout = new QVBoxLayout(page.widget);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    QWidget* title =
      makeTitle(tr("Use Print Screen with Phramer"), page.widget, 1.8);
    auto* subtitle = new QLabel(
      tr("Let Phramer answer when Windows is asked for a screen snip."),
      page.widget);
    subtitle->setObjectName(QStringLiteral("muted"));
    layout->addWidget(title);
    layout->addWidget(subtitle);
    layout->addSpacing(14);

    // Animates itself, so it is not part of the reveal
    m_zoom = new PrintScreenZoom(page.widget);
    layout->addWidget(m_zoom);
    layout->addSpacing(14);

    auto* detail = new QLabel(
      tr("Registering lists Phramer in Windows Settings as an MS-SCREENCLIP "
         "app. Choose it there, and the Print Screen key and apps that ask "
         "Windows for a snip open Phramer instead of the Snipping Tool."),
      page.widget);
    detail->setObjectName(QStringLiteral("muted"));
    detail->setWordWrap(true);
    layout->addWidget(detail);
    layout->addSpacing(10);

    m_screenClipBox = new QCheckBox(
      tr("Register Phramer as MS-SCREENCLIP (Administrator privileges "
         "required)"),
      page.widget);
    m_screenClipBox->setCursor(Qt::PointingHandCursor);
    // Off by default: it changes a machine-wide setting and asks for
    // administrator approval, which has to be the user's choice
    m_screenClipBox->setChecked(false);
    if (ScreenClipProtocol::isRegistered()) {
        m_screenClipBox->setText(tr("Phramer is registered as MS-SCREENCLIP"));
        m_screenClipBox->setChecked(true);
        m_screenClipBox->setEnabled(false);
    }
    layout->addWidget(m_screenClipBox);

    m_screenClipStatus = new QLabel(page.widget);
    m_screenClipStatus->setObjectName(QStringLiteral("muted"));
    m_screenClipStatus->setWordWrap(true);
    m_screenClipStatus->setTextFormat(Qt::RichText);
    connect(m_screenClipStatus, &QLabel::linkActivated, this, []() {
        ScreenClipProtocol::openDefaultAppsSettings();
    });
    m_screenClipStatus->hide();
    layout->addWidget(m_screenClipStatus);
    // Registered on an earlier run, but the choice in Settings never made
    if (ScreenClipProtocol::isRegistered() &&
        !ScreenClipProtocol::isDefault()) {
        m_screenClipStatus->setText(screenClipFinishHint());
        m_screenClipStatus->show();
    }

    auto* later = new QLabel(
      tr("You can change this at any time in Settings, under Advanced."),
      page.widget);
    later->setObjectName(QStringLiteral("muted"));
    QFont small = later->font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    later->setFont(small);
    layout->addSpacing(4);
    layout->addWidget(later);
    layout->addStretch(1);

    page.reveal = { title, subtitle, detail, m_screenClipBox, later };
    m_pages.append(page);
}
#endif

void WelcomeTour::buildFeaturesPage()
{
    Page page;
    page.widget = new QWidget(m_content);
    auto* layout = new QVBoxLayout(page.widget);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    QWidget* title = makeTitle(tr("What's new"), page.widget, 1.8);
    auto* subtitle =
      new QLabel(tr("New features and a redesigned experience."), page.widget);
    subtitle->setObjectName(QStringLiteral("muted"));
    layout->addWidget(title);
    layout->addWidget(subtitle);
    layout->addSpacing(14);
    page.reveal = { title, subtitle };

    struct Item
    {
        QString icon;
        QString title;
        QString body;
    };
    const QVector<Item> items = {
        { QStringLiteral("open-in-editor.svg"),
          tr("A redesigned editor"),
          tr("Light and dark themes that follow Windows, clearer tools and "
             "smoother motion.") },
        { QStringLiteral("pencil.svg"),
          tr("New tools"),
          tr("More tools to annotate and edit, each named with its key.") },
        { QStringLiteral("config.svg"),
          tr("Simpler settings"),
          tr("The essentials on General; everything else under Advanced.") },
        { QStringLiteral("pan-tool.svg"),
          tr("Keep editing after capture"),
          tr("Annotations drawn on the capture screen stay editable in the "
             "editor.") },
        { QStringLiteral("pixelate.svg"),
          tr("Frosted blur"),
          tr("A cleaner blur that never reads what it hides.") },
        { QStringLiteral("grid.svg"),
          tr("Zoom and grid"),
          tr("Ctrl+wheel to zoom, and grid lines that are never saved.") },
    };

    // Two columns, filled row by row
    auto* grid = new QGridLayout;
    grid->setHorizontalSpacing(28);
    grid->setVerticalSpacing(6);
    for (int i = 0; i < items.size(); ++i) {
        QWidget* row = makeRow(
          items.at(i).icon, items.at(i).title, items.at(i).body, page.widget);
        grid->addWidget(row, i / 2, i % 2);
        page.reveal.append(row);
    }
    layout->addLayout(grid);

    layout->addStretch(1);
    m_pages.append(page);
}

void WelcomeTour::buildFixesPage()
{
    Page page;
    page.widget = new QWidget(m_content);
    auto* layout = new QVBoxLayout(page.widget);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    QWidget* title =
      makeTitle(tr("Bug fixes and improvements"), page.widget, 1.8);
    auto* subtitle =
      new QLabel(tr("Smaller things that make a difference."), page.widget);
    subtitle->setObjectName(QStringLiteral("muted"));
    layout->addWidget(title);
    layout->addWidget(subtitle);
    layout->addSpacing(14);
    page.reveal = { title, subtitle };

    const QStringList fixes = {
        tr("Launch at startup turns on reliably when you tick it."),
        tr("Opening Settings in a second copy of Phramer no longer takes over "
           "Launch at startup."),
        tr("Blur lines up exactly at 125% and 150% display scaling."),
        tr("Sensible defaults: captures save straight to your folder and "
           "notifications stay quiet."),
        tr("Snip across all monitors is now the default capture mode."),
    };
    for (const QString& fix : fixes) {
        QWidget* row =
          makeRow(QStringLiteral("accept.svg"), fix, QString(), page.widget);
        layout->addWidget(row);
        page.reveal.append(row);
    }
    layout->addStretch(1);
    m_pages.append(page);
}

void WelcomeTour::buildFooter()
{
    auto* footer = new QWidget(m_card);
    auto* row = new QHBoxLayout(footer);
    row->setContentsMargins(0, 16, 0, 0);
    row->setSpacing(8);

    // Page pips: the current one is a longer bar
    auto* pips = new QHBoxLayout;
    pips->setSpacing(6);
    for (int i = 0; i < m_pages.size(); ++i) {
        auto* pip = new QLabel(footer);
        m_pips.append(pip);
        pips->addWidget(pip, 0, Qt::AlignVCenter);
    }
    row->addLayout(pips);
    row->addStretch(1);

    m_skipButton = new QPushButton(tr("Skip"), footer);
    m_skipButton->setObjectName(QStringLiteral("ghost"));
    m_nextButton = new QPushButton(tr("Next  ↵"), footer);
    m_nextButton->setObjectName(QStringLiteral("primary"));

    m_settingsButton = new QPushButton(tr("Phramer Settings"), footer);
    m_editorButton = new QPushButton(tr("Take me to Phramer  ↵"), footer);
    m_editorButton->setObjectName(QStringLiteral("primary"));

    for (QPushButton* button :
         { m_skipButton, m_nextButton, m_settingsButton, m_editorButton }) {
        button->setCursor(Qt::PointingHandCursor);
        // Enter is handled in keyPressEvent the same way on every page
        button->setAutoDefault(false);
        row->addWidget(button);
    }

    connect(m_skipButton, &QPushButton::clicked, this, &QDialog::accept);
    connect(m_nextButton, &QPushButton::clicked, this, [this]() { advance(); });
    connect(m_settingsButton,
            &QPushButton::clicked,
            this,
            &WelcomeTour::openSettings);
    connect(
      m_editorButton, &QPushButton::clicked, this, &WelcomeTour::openEditor);

    m_card->layout()->addWidget(footer);
    updateFooter();
}

void WelcomeTour::showEvent(QShowEvent* event)
{
    QDialog::showEvent(event);
    if (m_index >= 0) {
        return;
    }
    // A short fade for the window while the first page plays
    setWindowOpacity(0.0);
    auto* fade = new QVariantAnimation(this);
    fade->setStartValue(0.0);
    fade->setEndValue(1.0);
    fade->setDuration(220);
    fade->setEasingCurve(QEasingCurve::OutCubic);
    connect(fade,
            &QVariantAnimation::valueChanged,
            this,
            [this](const QVariant& v) { setWindowOpacity(v.toReal()); });
    fade->start(QAbstractAnimation::DeleteWhenStopped);
    goTo(0);
}

void WelcomeTour::goTo(int index)
{
    index = qBound(0, index, int(m_pages.size()) - 1);
    if (index == m_index) {
        return;
    }
    auto* stack = static_cast<QStackedLayout*>(m_content->layout());

    const auto enter = [this, stack, index]() {
        stack->setCurrentIndex(index);
        if (index == 0) {
            m_logo->play();
        } else {
            m_logo->stop();
        }
        if (m_zoom) {
            if (m_pages.at(index).widget == m_screenClipPage) {
                m_zoom->play();
            } else {
                m_zoom->stop();
            }
        }
        reveal(m_pages.at(index));
    };

    const int previous = m_index;
    // The new index is taken now, so a second Enter during the fade moves on
    // from the right page instead of repeating this one
    m_index = index;
    updateFooter();

    if (previous < 0) {
        enter();
        return;
    }

    // Fade the outgoing page's items together, then bring the next one in
    m_revealAnimation->stop();
    auto* out = new QParallelAnimationGroup(this);
    for (QWidget* item : m_pages.at(previous).reveal) {
        auto* effect =
          static_cast<QGraphicsOpacityEffect*>(item->graphicsEffect());
        auto* anim = new QVariantAnimation(out);
        anim->setStartValue(effect->opacity());
        anim->setEndValue(0.0);
        anim->setDuration(FadeOutMs);
        connect(
          anim,
          &QVariantAnimation::valueChanged,
          effect,
          [effect](const QVariant& v) { effect->setOpacity(v.toReal()); });
        out->addAnimation(anim);
    }
    connect(out, &QAbstractAnimation::finished, this, [this, enter, index]() {
        // A later goTo already moved on; entering this page now would show
        // the wrong one
        if (m_index == index) {
            enter();
        }
    });
    out->start(QAbstractAnimation::DeleteWhenStopped);
}

void WelcomeTour::reveal(const Page& page)
{
    m_revealAnimation->stop();
    m_revealAnimation->clear();
    // The first page waits for the logo to land before its text appears
    const int lead = page.widget == m_pages.first().widget ? 380 : 0;
    for (int i = 0; i < page.reveal.size(); ++i) {
        auto* effect = static_cast<QGraphicsOpacityEffect*>(
          page.reveal.at(i)->graphicsEffect());
        effect->setOpacity(0.0);

        auto* sequence = new QSequentialAnimationGroup(m_revealAnimation);
        sequence->addPause(lead + i * RevealStaggerMs);
        auto* anim = new QVariantAnimation(sequence);
        anim->setStartValue(0.0);
        anim->setEndValue(1.0);
        anim->setDuration(RevealMs);
        anim->setEasingCurve(QEasingCurve::OutCubic);
        connect(
          anim,
          &QVariantAnimation::valueChanged,
          effect,
          [effect](const QVariant& v) { effect->setOpacity(v.toReal()); });
        sequence->addAnimation(anim);
    }
    m_revealAnimation->start();
}

void WelcomeTour::updateFooter()
{
    const bool last = m_index == m_pages.size() - 1;
    m_skipButton->setVisible(!last);
    m_nextButton->setVisible(!last);
    m_settingsButton->setVisible(last);
    m_editorButton->setVisible(last);

    const QColor idle =
      m_dark ? QColor(255, 255, 255, 60) : QColor(0, 0, 0, 45);
    for (int i = 0; i < m_pips.size(); ++i) {
        const bool current = i == qMax(0, m_index);
        m_pips.at(i)->setFixedSize(current ? 22 : 7, 7);
        m_pips.at(i)->setStyleSheet(
          QStringLiteral("QLabel { background: %1; border-radius: 3px; }")
            .arg(rgba(current ? brandBlue() : idle)));
    }
}

void WelcomeTour::advance()
{
    if (m_index >= m_pages.size() - 1) {
        openEditor();
        return;
    }
    if (m_pages.at(m_index).widget == m_screenClipPage &&
        !registerScreenClipIfChecked()) {
        return;
    }
    goTo(m_index + 1);
}

QString WelcomeTour::screenClipFinishHint() const
{
    return tr("Last step: in Windows Settings, choose Phramer for "
              "MS-SCREENCLIP. %1")
      .arg(QStringLiteral("<a href=\"settings\" style=\"color: %1;\">%2</a>")
             .arg(brandBlue().name(), tr("Open Windows Settings")));
}

bool WelcomeTour::registerScreenClipIfChecked()
{
#if defined(Q_OS_WIN)
    // Disabled means it is already registered, by now or before
    if (!m_screenClipBox || !m_screenClipBox->isEnabled() ||
        !m_screenClipBox->isChecked()) {
        return true;
    }

    m_screenClipStatus->setText(tr("Waiting for administrator approval..."));
    m_screenClipStatus->show();
    // The UAC prompt blocks this thread, so paint the message first
    m_screenClipStatus->repaint();

    switch (ScreenClipProtocol::registerElevated()) {
        case ScreenClipProtocol::Result::Succeeded:
            m_screenClipBox->setText(
              tr("Phramer is registered as MS-SCREENCLIP"));
            m_screenClipBox->setEnabled(false);
            if (ScreenClipProtocol::isDefault()) {
                m_screenClipStatus->hide();
                return true;
            }
            // Only the user can make the choice, so hand them the page for
            // it. The tour stays put so the hint is read; the box is now
            // disabled, and the next Next moves on.
            m_screenClipStatus->setText(screenClipFinishHint());
            ScreenClipProtocol::openDefaultAppsSettings();
            return false;
        case ScreenClipProtocol::Result::Cancelled:
            m_screenClipStatus->setText(
              tr("Administrator approval was declined. Press Next to try "
                 "again, or untick the box to continue without it."));
            break;
        case ScreenClipProtocol::Result::Failed:
            m_screenClipStatus->setText(
              tr("Phramer could not be registered. Press Next to try again, "
                 "or untick the box to continue without it."));
            break;
    }
    // UAC takes focus away; bring the tour back so Enter reaches it
    raise();
    activateWindow();
    return false;
#else
    return true;
#endif
}

void WelcomeTour::openEditor()
{
    accept();
    // After the dialog is gone, so the editor is not raised beneath it
    QTimer::singleShot(0, []() { EditorWindow::openEmpty(); });
}

void WelcomeTour::openSettings()
{
    accept();
    QTimer::singleShot(0, []() { Flameshot::instance()->config(); });
}

void WelcomeTour::paintEvent(QPaintEvent* event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);

    const QRectF card(ShadowMargin, ShadowMargin, CardWidth, CardHeight);

    // A soft shadow from a few widening, fading outlines: cheaper than a
    // graphics effect, and it never blurs the card's own contents
    painter.setPen(Qt::NoPen);
    for (int i = ShadowMargin; i > 0; i -= 2) {
        const qreal t = qreal(i) / ShadowMargin;
        painter.setBrush(QColor(0, 0, 0, int(22 * (1.0 - t) * (1.0 - t))));
        painter.drawRoundedRect(
          card.adjusted(-i, -i + 6, i, i + 6), CardRadius + i, CardRadius + i);
    }

    const EditorTheme theme =
      m_dark ? EditorTheme::darkTheme() : EditorTheme::light();
    QPainterPath path;
    path.addRoundedRect(card, CardRadius, CardRadius);
    painter.fillPath(path, theme.window);
    painter.setPen(QPen(theme.border, 1));
    painter.drawPath(path);
}

void WelcomeTour::keyPressEvent(QKeyEvent* event)
{
    switch (event->key()) {
        case Qt::Key_Return:
        case Qt::Key_Enter:
            advance();
            event->accept();
            return;
        case Qt::Key_Right:
            if (m_index < m_pages.size() - 1) {
                goTo(m_index + 1);
            }
            event->accept();
            return;
        case Qt::Key_Left:
            goTo(m_index - 1);
            event->accept();
            return;
        default:
            break;
    }
    // Escape reaches QDialog, which rejects: the same as Skip
    QDialog::keyPressEvent(event);
}

void WelcomeTour::mousePressEvent(QMouseEvent* event)
{
    // A frameless window has no title bar to drag, so the card is the handle
    if (event->button() == Qt::LeftButton) {
        m_dragOffset =
          event->globalPosition().toPoint() - frameGeometry().topLeft();
        event->accept();
        return;
    }
    QDialog::mousePressEvent(event);
}

void WelcomeTour::mouseMoveEvent(QMouseEvent* event)
{
    if (!m_dragOffset.isNull() && (event->buttons() & Qt::LeftButton)) {
        move(event->globalPosition().toPoint() - m_dragOffset);
        event->accept();
        return;
    }
    QDialog::mouseMoveEvent(event);
}

void WelcomeTour::mouseReleaseEvent(QMouseEvent* event)
{
    m_dragOffset = QPoint();
    QDialog::mouseReleaseEvent(event);
}

bool WelcomeTour::showIfDue(QWidget* parent)
{
    ConfigHandler config;
    const QString current = QStringLiteral(APP_VERSION);
    const QString shown = config.welcomeTourShownFor();
    if (shown == current) {
        return false;
    }
    // "Don't show again" silences release notes, but a new major version is
    // a new experience and is introduced once to everyone
    const bool newMajor = majorOf(shown) != majorOf(current);
    if (config.welcomeTourDisabled() && !newMajor) {
        return false;
    }

    // Recorded before the dialog runs: closing it any way at all has to
    // count as seen, or it reappears on every launch
    config.setWelcomeTourShownFor(current);
    showNow(parent);
    return true;
}

void WelcomeTour::showNow(QWidget* parent)
{
    // One at a time: the tray entry can be clicked while one is open
    static QPointer<WelcomeTour> open;
    if (open) {
        open->raise();
        open->activateWindow();
        return;
    }
    open = new WelcomeTour(parent);
    open->setAttribute(Qt::WA_DeleteOnClose);
    open->show();
    open->activateWindow();
}
