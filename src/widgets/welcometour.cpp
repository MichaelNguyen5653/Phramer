// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/welcometour.h"

#include "utils/confighandler.h"
#include "utils/globalvalues.h"

#include <QCheckBox>
#include <QEasingCurve>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QMouseEvent>
#include <QPixmap>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QResizeEvent>
#include <QVBoxLayout>

namespace {

constexpr int SlideWidth = 720;
constexpr int SlideHeight = 430;
constexpr int TransitionMs = 320;
// A drag has to clear this much of a slide before it counts as a swipe
// rather than a stray click
constexpr double SwipeFraction = 0.18;

QColor brandBlue()
{
    return { 56, 153, 194 };
}

} // namespace

WelcomeTour::WelcomeTour(QWidget* parent)
  : QDialog(parent)
{
    setWindowTitle(tr("Welcome to Phramer"));
    setWindowIcon(GlobalValues::appIcon());
    setModal(true);
    // No context-help button, and no resize: the carousel is built around a
    // fixed slide width
    setWindowFlags(windowFlags() & ~Qt::WindowContextHelpButtonHint);

    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_viewport = new QWidget(this);
    m_viewport->setFixedSize(SlideWidth, SlideHeight);
    // Everything outside the current slide has to be clipped away, or the
    // neighbours show through during the animation
    m_viewport->setAutoFillBackground(true);
    layout->addWidget(m_viewport);

    m_track = new QWidget(m_viewport);
    buildSlides();
    layoutTrack();

    buildChrome();

    m_slideAnimation = new QPropertyAnimation(m_track, "pos", this);
    m_slideAnimation->setDuration(TransitionMs);
    m_slideAnimation->setEasingCurve(QEasingCurve::OutCubic);

    updateChrome();
    setFixedSize(sizeHint());
}

void WelcomeTour::buildSlides()
{
    m_slides.append(makeSlide(
      tr("Welcome to Phramer"),
      tr("Your screenshot tool has a new name, a new look and a new editor.\n"
         "Swipe or press Next to see what's new."),
      QStringLiteral(":img/app/appicon-512.png"),
      {}));

    m_slides.append(makeSlide(
      tr("Send any capture to the editor"),
      tr("Every capture can now be opened in a full editor window instead of "
         "being copied or saved straight away. Turn on \"Automatically open "
         "captures in Editor\" in Settings to make it the default."),
      QStringLiteral(":img/preview/Welcome_message_editor_feature.png"),
      {}));

    m_slides.append(makeSlide(
      tr("One window, every capture"),
      tr("The editor holds a whole session of images at once."),
      QStringLiteral(":img/preview/Welcome_message_editor_walkthrough.png"),
      { tr("The full suite of annotation tools, plus Copy, Save "
           "and Save All to export a whole session at once"),
        tr("Enhanced OCR — extract text from any image in the "
           "session, with layout preserved"),
        tr("Persistent preview — a filmstrip of every capture, so "
           "you can jump between them and keep editing") }));

    m_slides.append(
      makeSlide(tr("Also in this release"),
                QString(),
                QString(),
                { tr("UI/UX update — a new look built around the Phramer "
                     "blue, and shape tools condensed into one picker"),
                  tr("Bugs fixed — per-tool sizes no longer overwrite each "
                     "other, and highlights stay visible on dark captures"),
                  tr("Settings overhaul — the options you reach for are at "
                     "the top, and a search box finds the rest"),
                  tr("And more to come …") }));

    for (QWidget* slide : m_slides) {
        slide->setParent(m_track);
    }
}

QWidget* WelcomeTour::makeSlide(const QString& title,
                                const QString& body,
                                const QString& imageResource,
                                const QStringList& bullets)
{
    auto* slide = new QWidget;
    slide->setFixedSize(SlideWidth, SlideHeight);

    auto* layout = new QVBoxLayout(slide);
    layout->setContentsMargins(48, 40, 48, 24);
    layout->setSpacing(14);

    auto* titleLabel = new QLabel(title, slide);
    QFont titleFont = titleLabel->font();
    titleFont.setPointSizeF(titleFont.pointSizeF() * 1.7);
    titleFont.setBold(true);
    titleLabel->setFont(titleFont);
    titleLabel->setStyleSheet(
      QStringLiteral("color: %1;").arg(brandBlue().name()));
    titleLabel->setWordWrap(true);
    layout->addWidget(titleLabel);

    if (!body.isEmpty()) {
        auto* bodyLabel = new QLabel(body, slide);
        bodyLabel->setWordWrap(true);
        layout->addWidget(bodyLabel);
    }

    if (!imageResource.isEmpty()) {
        auto* imageLabel = new QLabel(slide);
        QPixmap image(imageResource);
        if (!image.isNull()) {
            // Scaled to the space that is left rather than a fixed size, so a
            // slide carrying bullets as well still fits
            const int maxHeight = bullets.isEmpty() ? 250 : 170;
            imageLabel->setPixmap(image.scaled(SlideWidth - 96,
                                               maxHeight,
                                               Qt::KeepAspectRatio,
                                               Qt::SmoothTransformation));
        }
        imageLabel->setAlignment(Qt::AlignCenter);
        layout->addWidget(imageLabel);
    }

    for (const QString& bullet : bullets) {
        auto* row = new QHBoxLayout;
        row->setSpacing(10);

        auto* marker = new QLabel(QStringLiteral("●"), slide);
        marker->setStyleSheet(
          QStringLiteral("color: %1;").arg(brandBlue().name()));
        marker->setAlignment(Qt::AlignTop | Qt::AlignLeft);

        auto* text = new QLabel(bullet, slide);
        text->setWordWrap(true);

        row->addWidget(marker);
        row->addWidget(text, 1);
        layout->addLayout(row);
    }

    layout->addStretch();
    return slide;
}

void WelcomeTour::buildChrome()
{
    auto* container = new QWidget(this);
    auto* outer = new QVBoxLayout(container);
    outer->setContentsMargins(24, 12, 24, 18);
    outer->setSpacing(12);

    auto* dotRow = new QHBoxLayout;
    dotRow->addStretch();
    for (int i = 0; i < m_slides.size(); ++i) {
        auto* dot = new QLabel(QStringLiteral("●"), container);
        dot->setCursor(Qt::PointingHandCursor);
        dot->setToolTip(tr("Go to slide %1").arg(i + 1));
        m_dots.append(dot);
        dotRow->addWidget(dot);
    }
    dotRow->addStretch();
    outer->addLayout(dotRow);

    auto* buttonRow = new QHBoxLayout;
    m_neverAgain = new QCheckBox(tr("Don't show this again"), container);
    // Off by default: a one-off tour that also silences future release notes
    // should be an explicit choice
    m_neverAgain->setChecked(false);
    buttonRow->addWidget(m_neverAgain);
    buttonRow->addStretch();

    m_skipButton = new QPushButton(tr("Skip"), container);
    m_backButton = new QPushButton(tr("Back"), container);
    m_nextButton = new QPushButton(tr("Next"), container);
    m_nextButton->setDefault(true);
    m_nextButton->setStyleSheet(
      QStringLiteral("QPushButton { background-color: %1; color: white; "
                     "border: none; padding: 6px 18px; border-radius: 4px; }"
                     "QPushButton:hover { background-color: %2; }")
        .arg(brandBlue().name(), brandBlue().darker(115).name()));

    connect(m_skipButton, &QPushButton::clicked, this, &QDialog::accept);
    connect(m_backButton, &QPushButton::clicked, this, [this]() {
        goTo(m_index - 1);
    });
    connect(m_nextButton, &QPushButton::clicked, this, [this]() {
        if (m_index == m_slides.size() - 1) {
            accept();
        } else {
            goTo(m_index + 1);
        }
    });

    buttonRow->addWidget(m_skipButton);
    buttonRow->addWidget(m_backButton);
    buttonRow->addWidget(m_nextButton);
    outer->addLayout(buttonRow);

    layout()->addWidget(container);
}

void WelcomeTour::layoutTrack()
{
    m_track->setFixedSize(SlideWidth * m_slides.size(), SlideHeight);
    for (int i = 0; i < m_slides.size(); ++i) {
        m_slides.at(i)->move(i * SlideWidth, 0);
    }
    m_track->move(-m_index * SlideWidth, 0);
}

void WelcomeTour::goTo(int index, bool animate)
{
    index = qBound(0, index, m_slides.size() - 1);
    m_index = index;
    const QPoint target(-index * SlideWidth, 0);

    m_slideAnimation->stop();
    if (animate) {
        m_slideAnimation->setStartValue(m_track->pos());
        m_slideAnimation->setEndValue(target);
        m_slideAnimation->start();
    } else {
        m_track->move(target);
    }
    updateChrome();
}

void WelcomeTour::updateChrome()
{
    m_backButton->setEnabled(m_index > 0);
    const bool last = m_index == m_slides.size() - 1;
    m_nextButton->setText(last ? tr("Get started") : tr("Next"));
    // Skipping and finishing do the same thing, so the button is noise on
    // the final slide
    m_skipButton->setVisible(!last);

    for (int i = 0; i < m_dots.size(); ++i) {
        const QColor colour =
          i == m_index ? brandBlue() : palette().color(QPalette::Mid);
        m_dots.at(i)->setStyleSheet(
          QStringLiteral("color: %1; font-size: %2px;")
            .arg(colour.name())
            .arg(i == m_index ? 15 : 11));
    }
}

void WelcomeTour::keyPressEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_Right) {
        goTo(m_index + 1);
        event->accept();
        return;
    }
    if (event->key() == Qt::Key_Left) {
        goTo(m_index - 1);
        event->accept();
        return;
    }
    QDialog::keyPressEvent(event);
}

void WelcomeTour::mousePressEvent(QMouseEvent* event)
{
    // Only drags that start over the slide area are swipes; the button row
    // and checkbox below must keep behaving normally
    if (event->button() == Qt::LeftButton &&
        m_viewport->geometry().contains(event->pos())) {
        m_dragStartX = event->pos().x();
        m_dragTrackX = m_track->x();
        m_slideAnimation->stop();
        return;
    }
    QDialog::mousePressEvent(event);
}

void WelcomeTour::mouseMoveEvent(QMouseEvent* event)
{
    if (m_dragStartX < 0) {
        QDialog::mouseMoveEvent(event);
        return;
    }
    const int delta = event->pos().x() - m_dragStartX;
    // Clamped to the ends so the first and last slides cannot be dragged
    // off into empty space
    const int minX = -(m_slides.size() - 1) * SlideWidth;
    m_track->move(qBound(minX, m_dragTrackX + delta, 0), 0);
}

void WelcomeTour::mouseReleaseEvent(QMouseEvent* event)
{
    if (m_dragStartX < 0) {
        QDialog::mouseReleaseEvent(event);
        return;
    }
    const int delta = event->pos().x() - m_dragStartX;
    m_dragStartX = -1;

    const int threshold = int(SlideWidth * SwipeFraction);
    if (delta <= -threshold) {
        goTo(m_index + 1);
    } else if (delta >= threshold) {
        goTo(m_index - 1);
    } else {
        // Not far enough: settle back onto the slide it came from
        goTo(m_index);
    }
}

void WelcomeTour::resizeEvent(QResizeEvent* event)
{
    QDialog::resizeEvent(event);
    layoutTrack();
    goTo(m_index, false);
}

void WelcomeTour::done(int result)
{
    if (m_neverAgain && m_neverAgain->isChecked()) {
        ConfigHandler().setWelcomeTourDisabled(true);
    }
    QDialog::done(result);
}

bool WelcomeTour::showIfDue(QWidget* parent)
{
    ConfigHandler config;
    if (config.welcomeTourDisabled()) {
        return false;
    }
    if (config.welcomeTourShownFor() == QStringLiteral(APP_VERSION)) {
        return false;
    }

    // Recorded before the dialog runs: closing it with the window controls
    // has to count as seen, or it reappears on every launch
    config.setWelcomeTourShownFor(QStringLiteral(APP_VERSION));

    auto* tour = new WelcomeTour(parent);
    tour->setAttribute(Qt::WA_DeleteOnClose);
    tour->show();
    return true;
}
