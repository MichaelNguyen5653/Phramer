// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrscore.h"

namespace {

// Structure is worth something on its own: thirty short lines is a page,
// one long line of the same length is usually a misread
constexpr int LineBonus = 2;

// Enough to cancel a lone mark's own character, and its line's bonus over
// four such lines
constexpr int JunkPenalty = 2;

} // namespace

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
