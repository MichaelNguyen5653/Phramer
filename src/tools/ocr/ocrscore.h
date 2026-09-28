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

/**
 * @brief Whether text read from a live window matches what was recognized
 * in the screenshot.
 *
 * UI Automation reads whichever window is on screen when OCR runs, not the
 * frozen pixels, and happily returns text from a window hidden behind the
 * one captured, or whole lines that run past the selection. Recognition is
 * the only witness to what the capture actually shows, so exact text is
 * trusted only when most of its words appear in the recognized text and
 * most of the recognized words appear in it, and at least three words are
 * shared -- a word or two matches too many windows. Case and punctuation are
 * ignored; a few misread words are tolerated, which is the point of
 * preferring exact text at all.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
bool ocrTextsAgree(const QString& recognized, const QString& exact);
