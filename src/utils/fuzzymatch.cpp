// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/fuzzymatch.h"

#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QVector>

namespace {

/**
 * @brief Levenshtein distance, abandoned once it is certain to exceed limit.
 *
 * The bound is what makes this cheap enough to run against every word of
 * every row on each keystroke.
 */
int boundedEditDistance(const QString& a, const QString& b, int limit)
{
    if (qAbs(a.size() - b.size()) > limit) {
        return limit + 1;
    }
    QVector<int> previous(b.size() + 1);
    QVector<int> current(b.size() + 1);
    for (int j = 0; j <= b.size(); ++j) {
        previous[j] = j;
    }
    for (int i = 1; i <= a.size(); ++i) {
        current[0] = i;
        int rowBest = current[0];
        for (int j = 1; j <= b.size(); ++j) {
            const int cost = (a.at(i - 1) == b.at(j - 1)) ? 0 : 1;
            current[j] = qMin(qMin(current[j - 1] + 1, previous[j] + 1),
                              previous[j - 1] + cost);
            rowBest = qMin(rowBest, current[j]);
        }
        if (rowBest > limit) {
            return limit + 1;
        }
        previous = current;
    }
    return previous[b.size()];
}

// How wrong a word is allowed to be and still count as the same word
int typoBudget(int length)
{
    if (length <= 3) {
        return 1;
    }
    return length <= 6 ? 2 : 3;
}

// Every character of needle appears in haystack, in order. Catches run-on
// and abbreviated queries such as "trayicn" for "Show tray icon".
bool isSubsequence(const QString& needle, const QString& haystack)
{
    int index = 0;
    for (const QChar& c : haystack) {
        if (c == needle.at(index)) {
            if (++index == needle.size()) {
                return true;
            }
        }
    }
    return false;
}

} // namespace

bool FuzzyMatch::matches(const QString& query, const QString& text)
{
    const QString haystack = text.toLower();
    const QStringList terms =
      query.toLower().split(QRegularExpression("\\s+"), Qt::SkipEmptyParts);
    if (terms.isEmpty()) {
        return true;
    }

    const QStringList words =
      haystack.split(QRegularExpression("[^a-z0-9]+"), Qt::SkipEmptyParts);

    for (const QString& term : terms) {
        if (haystack.contains(term)) {
            continue;
        }
        bool matched = false;
        const int budget = typoBudget(term.size());
        for (const QString& word : words) {
            if (word.startsWith(term) ||
                boundedEditDistance(term, word, budget) <= budget) {
                matched = true;
                break;
            }
        }
        // Short terms would subsequence-match nearly everything
        if (!matched && term.size() >= 3 && isSubsequence(term, haystack)) {
            matched = true;
        }
        if (!matched) {
            return false;
        }
    }
    return true;
}
