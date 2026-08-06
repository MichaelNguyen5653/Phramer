// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

class QString;

/**
 * @brief Typo-tolerant matching for the settings search boxes.
 *
 * Deliberately not a substring test: settings and shortcuts get looked up by
 * half-remembered names, so every whitespace-separated term has to match the
 * label as a fragment, as a near-miss of one of its words, or as a
 * subsequence of the whole thing.
 */
namespace FuzzyMatch {

bool matches(const QString& query, const QString& text);

}
