// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrscore.h"

#include <QHash>
#include <QStringList>

namespace {

// Structure is worth something on its own: thirty short lines is a page,
// one long line of the same length is usually a misread
constexpr int LineBonus = 2;

// Enough to cancel a lone mark's own character, and its line's bonus over
// four such lines
constexpr int JunkPenalty = 2;

// Share of words each side must find in the other. Low enough for the
// odd misread word, high enough that a different window or text past the
// selection edge fails it.
constexpr qreal MinWordAgreement = 0.7;

// Below this many shared words agreement proves nothing: "OK" or "2024"
// appears in plenty of windows the capture never showed
constexpr int MinSharedWords = 3;

QStringList wordsOf(const QString& text)
{
    QStringList words;
    QString current;
    for (const QChar c : text) {
        if (c.isLetterOrNumber()) {
            current.append(c.toLower());
        } else if (!current.isEmpty()) {
            words.append(current);
            current.clear();
        }
    }
    if (!current.isEmpty()) {
        words.append(current);
    }
    return words;
}

} // namespace

bool ocrTextsAgree(const QString& recognized, const QString& exact)
{
    const QStringList seen = wordsOf(recognized);
    const QStringList read = wordsOf(exact);
    if (seen.isEmpty() || read.isEmpty()) {
        return false;
    }

    // Multiset intersection, so a word repeated in one text is not matched
    // over and over against a single occurrence in the other
    QHash<QString, int> available;
    for (const QString& word : read) {
        ++available[word];
    }
    int matched = 0;
    for (const QString& word : seen) {
        auto it = available.find(word);
        if (it != available.end() && it.value() > 0) {
            --it.value();
            ++matched;
        }
    }

    return matched >= MinSharedWords &&
           matched >= MinWordAgreement * seen.size() &&
           matched >= MinWordAgreement * read.size();
}

int ocrScoreResult(const OcrResult& result)
{
    if (result.status != OcrResult::Status::Ok) {
        return 0;
    }

    int score = 0;
    for (const OcrLine& line : result.lines) {
        score += LineBonus;

        if (line.words.isEmpty()) {
            score += line.text.trimmed().size();
            continue;
        }

        for (const OcrWord& word : line.words) {
            const QString text = word.text.trimmed();
            score += text.size();
            if (text.size() == 1 && !text.at(0).isLetterOrNumber()) {
                score -= JunkPenalty;
            }
        }
    }
    return score;
}
