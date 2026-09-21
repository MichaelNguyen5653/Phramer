// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "texttool.h"
#include "tools/text/textconfig.h"
#include "tools/text/textwidget.h"
#include "utils/confighandler.h"

#define BASE_POINT_SIZE 8
#define MAX_INFO_LENGTH 24

// Blank space kept between the glyphs and the edge of m_textArea, which is
// both the object's selection outline and its grabbable area.
static constexpr int kTextPadding = 5;

TextTool::TextTool(QObject* parent)
  : CaptureTool(parent)
  , m_size(1)
{
    QString fontFamily = ConfigHandler().fontFamily();
    if (!fontFamily.isEmpty()) {
        m_font.setFamily(ConfigHandler().fontFamily());
    }
    m_alignment = Qt::AlignLeft;
}

TextTool::~TextTool()
{
    closeEditor();
}

void TextTool::copyParams(const TextTool* from, TextTool* to)
{
    CaptureTool::copyParams(from, to);
    to->m_font = from->m_font;
    to->m_alignment = from->m_alignment;
    to->m_text = from->m_text;
    to->m_size = from->m_size;
    to->m_color = from->m_color;
    to->m_textArea = from->m_textArea;
    to->m_currentPos = from->m_currentPos;
}

bool TextTool::isValid() const
{
    return !m_text.isEmpty();
}

bool TextTool::closeOnButtonPressed() const
{
    return false;
}

bool TextTool::isSelectable() const
{
    return true;
}

bool TextTool::showMousePreview() const
{
    return false;
}

QRect TextTool::boundingRect() const
{
    return m_textArea;
}

QIcon TextTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "text.svg");
}

QString TextTool::name() const
{
    return tr("Text");
}

QString TextTool::info()
{
    if (m_text.length() > 0) {
        m_tempString = QString("%1 - %2").arg(name(), m_text.trimmed());
        m_tempString = m_tempString.split("\n").at(0);
        if (m_tempString.length() > MAX_INFO_LENGTH) {
            m_tempString.truncate(MAX_INFO_LENGTH);
            m_tempString += "…";
        }
        return m_tempString;
    }
    return name();
}

CaptureTool::Type TextTool::type() const
{
    return CaptureTool::TYPE_TEXT;
}

QString TextTool::description() const
{
    return tr("Add text to your capture");
}

QWidget* TextTool::widget()
{
    closeEditor();
    m_widget = new TextWidget();
    m_widget->setTextColor(m_color);
    m_font.setPointSize(m_size + BASE_POINT_SIZE);
    applyWidgetFont();
    m_widget->setAlignment(m_alignment);
    m_widget->setText(m_text);
    m_widget->selectAll();
    connect(m_widget, &TextWidget::textUpdated, this, &TextTool::updateText);
    connect(
      m_widget,
      &TextWidget::editingFinished,
      this,
      [this]() { emit requestAction(REQ_COMMIT_CURRENT_TOOL); },
      Qt::QueuedConnection);
    return m_widget;
}

// The editor widget is a real child widget, so the canvas's painter transform
// does not reach it. Scaling the point size is what makes the text being typed
// the same visual size as the text that will be committed.
void TextTool::applyWidgetFont()
{
    if (m_widget.isNull()) {
        return;
    }
    QFont scaled = m_font;
    scaled.setPointSizeF(m_font.pointSizeF() * m_editorScale);
    m_widget->setFont(scaled);
}

void TextTool::setEditorScale(qreal scale)
{
    if (qFuzzyCompare(scale, m_editorScale)) {
        return;
    }
    m_editorScale = scale;
    applyWidgetFont();
}

void TextTool::closeEditor()
{
    if (!m_widget.isNull()) {
        m_widget->hide();
        delete m_widget;
        m_widget = nullptr;
    }
    if (!m_confW.isNull()) {
        m_confW->hide();
        delete m_confW;
        m_confW = nullptr;
    }
}

QWidget* TextTool::configurationWidget()
{
    m_confW = new TextConfig();
    connect(
      m_confW, &TextConfig::fontFamilyChanged, this, &TextTool::updateFamily);
    connect(m_confW,
            &TextConfig::fontItalicChanged,
            this,
            &TextTool::updateFontItalic);
    connect(m_confW,
            &TextConfig::fontStrikeOutChanged,
            this,
            &TextTool::updateFontStrikeOut);
    connect(m_confW,
            &TextConfig::fontUnderlineChanged,
            this,
            &TextTool::updateFontUnderline);
    connect(m_confW,
            &TextConfig::fontWeightChanged,
            this,
            &TextTool::updateFontWeight);

    connect(
      m_confW, &TextConfig::alignmentChanged, this, &TextTool::updateAlignment);

    m_confW->setFontFamily(m_font.family());
    m_confW->setItalic(m_font.italic());
    m_confW->setUnderline(m_font.underline());
    m_confW->setStrikeOut(m_font.strikeOut());
    m_confW->setWeight(m_font.weight());
    m_confW->setTextAlignment(m_alignment);
    return m_confW;
}

CaptureTool* TextTool::copy(QObject* parent)
{
    auto* textTool = new TextTool(parent);
    if (m_confW != nullptr) {
        connect(m_confW,
                &TextConfig::fontFamilyChanged,
                textTool,
                &TextTool::updateFamily);
        connect(m_confW,
                &TextConfig::fontItalicChanged,
                textTool,
                &TextTool::updateFontItalic);
        connect(m_confW,
                &TextConfig::fontStrikeOutChanged,
                textTool,
                &TextTool::updateFontStrikeOut);
        connect(m_confW,
                &TextConfig::fontUnderlineChanged,
                textTool,
                &TextTool::updateFontUnderline);
        connect(m_confW,
                &TextConfig::fontWeightChanged,
                textTool,
                &TextTool::updateFontWeight);

        connect(m_confW,
                &TextConfig::alignmentChanged,
                textTool,
                &TextTool::updateAlignment);
    }
    copyParams(this, textTool);
    return textTool;
}

void TextTool::process(QPainter& painter, const QPixmap& pixmap)
{
    Q_UNUSED(pixmap)
    if (m_text.isEmpty()) {
        return;
    }
    QFont orig_font = painter.font();
    QPen orig_pen = painter.pen();
    QFontMetrics fm(m_font);
    QSize fontsize(fm.boundingRect(QRect(), 0, m_text).size());
    fontsize.setWidth(fontsize.width() + kTextPadding * 2);
    fontsize.setHeight(fontsize.height() + kTextPadding * 2);
    m_textArea.setSize(fontsize);
    // draw text
    painter.setFont(m_font);
    painter.setPen(m_color);
    if (!editMode()) {
        painter.drawText(
          m_textArea +
            QMargins(-kTextPadding, -kTextPadding, kTextPadding, kTextPadding),
          m_alignment,
          m_text);
    }
    painter.setFont(orig_font);
    painter.setPen(orig_pen);

    if (m_widget != nullptr) {
        m_widget->setAlignment(m_alignment);
    }
}

// Picking tests the pixels a tool paints, and process() paints only glyphs.
// Without this the gaps between letters and the whole interior of the box
// are dead space, so the object can only be grabbed by landing on a letter
// stroke. Claiming the box makes the grabbable area match the outline
// drawObjectSelection draws, which is what the user is aiming at.
void TextTool::drawSearchArea(QPainter& painter, const QPixmap& pixmap)
{
    Q_UNUSED(pixmap)
    if (m_text.isEmpty()) {
        return;
    }
    painter.fillRect(m_textArea, Qt::black);
}

void TextTool::drawObjectSelection(QPainter& painter)
{
    if (m_text.isEmpty()) {
        return;
    }
    drawObjectSelectionRect(painter, boundingRect());
}

void TextTool::paintMousePreview(QPainter& painter,
                                 const CaptureContext& context)
{
    Q_UNUSED(painter)
    Q_UNUSED(context)
}

// Half a line of the current font. The click is an insertion point, so the
// first line has to straddle it; anchoring the top of the line there instead
// drops the text below the cursor, and the bigger the font the further it
// falls.
int TextTool::firstLineHalfHeight() const
{
    return QFontMetrics(m_font).height() / 2;
}

// m_textArea is the glyphs plus kTextPadding on every side, so back the
// padding out to leave the glyphs themselves starting at the anchor.
QPoint TextTool::textAreaTopLeft(const QPoint& anchor) const
{
    return { anchor.x() - kTextPadding,
             anchor.y() - kTextPadding - firstLineHalfHeight() };
}

// Maps m_textArea's corner onto the editor widget's corner, so the editor
// shows its glyphs exactly where the committed object will draw them. Without
// it the text shifts the moment the edit is committed.
QPoint TextTool::childWidgetOffset() const
{
    if (m_widget.isNull()) {
        return {};
    }
    const int padding = qRound(kTextPadding * m_editorScale);
    return QPoint(padding, padding) - m_widget->textOrigin();
}

void TextTool::drawEnd(const QPoint& point)
{
    m_textArea.moveTo(textAreaTopLeft(point));
}

void TextTool::drawMove(const QPoint& point)
{
    m_textArea.moveTo(textAreaTopLeft(point));
    // m_textArea is in image coordinates but the editor is a child widget, so
    // the scale has to be applied here. At 1.0, which is every case outside
    // the editor's zoom, this is the identity.
    const QPoint scaled(qRound(m_textArea.left() * m_editorScale),
                        qRound(m_textArea.top() * m_editorScale));
    m_widget->move(scaled + childWidgetOffset());
}

void TextTool::drawStart(const CaptureContext& context)
{
    m_color = context.color;
    m_size = context.toolSize;
    // The editor is positioned from m_textArea, and the anchor depends on the
    // font, so both have to be resolved before the child widget is created.
    m_font.setPointSize(m_size + BASE_POINT_SIZE);
    m_textArea.moveTo(textAreaTopLeft(context.mousePos));
    emit requestAction(REQ_ADD_CHILD_WIDGET);
}

void TextTool::pressed(CaptureContext& context)
{
    Q_UNUSED(context)
}

void TextTool::onColorChanged(const QColor& color)
{
    m_color = color;
    if (m_widget != nullptr) {
        m_widget->setTextColor(color);
    }
}

void TextTool::onSizeChanged(int size)
{
    m_size = size;
    m_font.setPointSize(m_size + BASE_POINT_SIZE);
    applyWidgetFont();
}

void TextTool::updateText(const QString& newText)
{
    m_text = newText;
}

void TextTool::updateFamily(const QString& text)
{
    m_font.setFamily(text);
    if (m_textOld.isEmpty()) {
        ConfigHandler().setFontFamily(m_font.family());
    }
    applyWidgetFont();
}

void TextTool::updateFontUnderline(const bool underlined)
{
    m_font.setUnderline(underlined);
    applyWidgetFont();
}

void TextTool::updateFontStrikeOut(const bool strikeout)
{
    m_font.setStrikeOut(strikeout);
    applyWidgetFont();
}

void TextTool::updateFontWeight(const QFont::Weight weight)
{
    m_font.setWeight(weight);
    applyWidgetFont();
}

void TextTool::updateFontItalic(const bool italic)
{
    m_font.setItalic(italic);
    applyWidgetFont();
}

void TextTool::move(const QPoint& pos)
{
    m_textArea.moveTo(pos);
}

void TextTool::updateAlignment(Qt::AlignmentFlag alignment)
{
    m_alignment = alignment;
    if (m_widget != nullptr) {
        m_widget->setAlignment(m_alignment);
    }
}

const QPoint* TextTool::pos()
{
    m_currentPos = m_textArea.topLeft();
    return &m_currentPos;
}

void TextTool::setEditMode(bool editMode)
{
    if (editMode) {
        m_textOld = m_text;
    }
    CaptureTool::setEditMode(editMode);
}

bool TextTool::isChanged()
{
    return QString::compare(m_text, m_textOld, Qt::CaseInsensitive) != 0;
}
