// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

/**
 * @brief How good a recognition result looks, in arbitrary points.
 *
 * Windows.Media.Ocr reports no per-word confidence, so the only signals
 * available are how much text came back and how plausibly it is shaped.
 * Character count dominates because the failure being guarded against is
 * "almost nothing came back"; the penalty on lone punctuation exists because
 * a badly scaled pass returns scattered marks, which would otherwise beat a
 * short clean result on line count alone.
 *
 * Only comparisons between scores are meaningful. A result that did not
 * succeed scores 0, so a failed pass can never displace a successful one.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
int ocrScoreResult(const OcrResult& result);
