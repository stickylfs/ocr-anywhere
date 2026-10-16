use windows::core::Result as WinResult;
use windows::Graphics::Imaging::SoftwareBitmap;
use windows::Media::Ocr::{OcrEngine, OcrResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizedText {
    pub text: String,
    pub language_tag: String,
    pub line_count: usize,
    pub word_count: usize,
}

/// Runs OCR on a SoftwareBitmap using the specified OcrEngine.
pub fn recognize_bitmap(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> WinResult<RecognizedText> {
    let async_op = engine.RecognizeAsync(bitmap)?;
    let result: OcrResult = async_op.get()?;

    let language_tag = engine
        .RecognizerLanguage()
        .map(|l| l.LanguageTag().unwrap_or_default().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let lines = result.Lines()?;
    let line_count = lines.Size()? as usize;

    let mut full_lines = Vec::with_capacity(line_count);
    let mut total_words = 0;

    for i in 0..line_count {
        let line = lines.GetAt(i as u32)?;
        let line_text = line.Text()?.to_string();
        let words = line.Words()?;
        total_words += words.Size()? as usize;
        full_lines.push(line_text);
    }

    let cleaned_text = format_ocr_lines(&full_lines);

    Ok(RecognizedText {
        text: cleaned_text,
        language_tag,
        line_count,
        word_count: total_words,
    })
}

/// Cleans and formats OCR lines into readable text preserving paragraphs and reasonable whitespace.
pub fn format_ocr_lines(raw_lines: &[String]) -> String {
    let mut formatted_lines: Vec<String> = Vec::new();

    for line in raw_lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let cleaned = clean_line_artifacts(trimmed);
            let normalized = normalize_ocr_timestamp(&cleaned);
            if !normalized.is_empty() {
                formatted_lines.push(normalized);
            }
        }
    }

    if formatted_lines.is_empty() {
        return String::new();
    }

    // CRLF is the line separator expected by Windows clipboard consumers.
    formatted_lines.join("\r\n")
}

/// Cleans leading icon artifacts that commonly appear in file lists or dialogs.
fn clean_line_artifacts(line: &str) -> String {
    let mut s = line.trim();

    // Strip common leading icon glyphs like "G] ", "[] ", "[+] ", "|] ", "l] ", "I] "
    // when the line doesn't contain a corresponding opening bracket '['.
    if !s.contains('[') {
        if let Some(rest) = s
            .strip_prefix("G] ")
            .or_else(|| s.strip_prefix("g] "))
            .or_else(|| s.strip_prefix("[] "))
            .or_else(|| s.strip_prefix("[+] "))
            .or_else(|| s.strip_prefix("|] "))
            .or_else(|| s.strip_prefix("l] "))
            .or_else(|| s.strip_prefix("I] "))
            .or_else(|| s.strip_prefix("O] "))
            .or_else(|| s.strip_prefix("0] "))
        {
            s = rest.trim_start();
        }
    }

    // Strip leading '@' when immediately attached to a filename with an extension (e.g. "@ocr-anywhere...exe")
    if let Some(rest) = s.strip_prefix('@') {
        if rest.starts_with(' ') {
            s = rest.trim_start();
        } else if rest.contains(".exe")
            || rest.contains(".dll")
            || rest.contains(".zip")
            || rest.contains(".png")
            || rest.contains(".jpg")
            || rest.contains(".pdf")
            || rest.contains(".txt")
        {
            s = rest;
        }
    }

    s.to_string()
}

/// Repairs times that follow a date, a pattern the recognizer often garbles in
/// file listings: the colon is read as another symbol (`16'04`, `16•04`), dropped
/// (`1604`), split (`1 5:49`) or the time is glued to the date (`09/09/20261603`).
/// Tokens are only rewritten right after a date, so ordinary numbers are untouched.
pub fn normalize_ocr_timestamp(line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return line.to_string();
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let w = words[i];

        // Check if word is date + time concatenated (e.g. "09/09/20261603")
        if let Some((date_part, time_part)) = split_concatenated_date_time(w) {
            result.push(date_part);
            if let Some(norm) = clean_time_token(&time_part) {
                result.push(norm);
            } else {
                result.push(time_part);
            }
            i += 1;
            continue;
        }

        let is_date = is_date_token(w);
        result.push(w.to_string());

        // If this token was a date, check if following token(s) represent a time
        if is_date && i + 1 < words.len() {
            let next = words[i + 1];

            // Case A: time token like "16'04", "16•04", "1604"
            if let Some(norm_time) = clean_time_token(next) {
                result.push(norm_time);
                i += 2;
                continue;
            }

            // Case B: split time like "1" "5:49" -> "15:49" or "15" "48" -> "15:48"
            if i + 2 < words.len() {
                let w2 = words[i + 2];
                if let Some(merged) = clean_split_time_tokens(next, w2) {
                    result.push(merged);
                    i += 3;
                    continue;
                }
            }
        }

        i += 1;
    }

    result.join(" ")
}

fn is_date_token(token: &str) -> bool {
    let clean = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '/' && c != '-');
    if clean.contains('/') {
        let parts: Vec<&str> = clean.split('/').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    } else if clean.contains('-') {
        let parts: Vec<&str> = clean.split('-').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    }
    false
}

fn split_concatenated_date_time(token: &str) -> Option<(String, String)> {
    if token.len() >= 14 && token.contains('/') {
        let last_slash = token.rfind('/')?;
        if token.len() >= last_slash + 5 + 3 {
            let year_end = last_slash + 5;
            let date_part = &token[..year_end];
            let time_part = &token[year_end..];
            if is_date_token(date_part) && time_part.chars().any(|c| c.is_ascii_digit()) {
                return Some((date_part.to_string(), time_part.to_string()));
            }
        }
    }
    None
}

fn clean_time_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|c: char| {
        !c.is_alphanumeric()
            && c != '&'
            && c != '\''
            && c != '•'
            && c != '<'
            && c != ':'
            && c != '.'
            && c != '-'
    });
    let char_count = trimmed.chars().count();
    if !(3..=6).contains(&char_count) {
        return None;
    }

    // Already clean HH:MM
    if let Some((h_str, m_str)) = trimmed.split_once(':') {
        if let (Ok(h), Ok(m)) = (h_str.parse::<u32>(), m_str.parse::<u32>()) {
            if h < 24 && m < 60 {
                return Some(format!("{:02}:{:02}", h, m));
            }
        }
    }

    let chars: Vec<char> = trimmed.chars().collect();

    // Colon misread as another symbol: "16'04", "16•04", "16;04", ...
    // ('.' and '-' are excluded: decimals and ranges are far more common).
    for sep in ['\'', '•', '<', ';', '`', '’', '·', '&'] {
        if let Some((h_str, m_str)) = trimmed.split_once(sep) {
            if let (Ok(h), Ok(m)) = (h_str.parse::<u32>(), m_str.parse::<u32>()) {
                if h < 24 && m < 60 {
                    return Some(format!("{:02}:{:02}", h, m));
                }
            }
        }
    }

    // 4 consecutive digits "1603", "1604", "1548"
    if chars.len() == 4 && chars.iter().all(|c| c.is_ascii_digit()) {
        let h: u32 = trimmed[0..2].parse().ok()?;
        let m: u32 = trimmed[2..4].parse().ok()?;
        if h < 24 && m < 60 {
            return Some(format!("{:02}:{:02}", h, m));
        }
    }

    None
}

fn clean_split_time_tokens(w1: &str, w2: &str) -> Option<String> {
    let t1 = w1.trim();
    let t2 = w2.trim();

    // e.g. "1" "5:49" -> "15:49"
    if t1 == "1" && t2.len() >= 4 && t2.contains(':') {
        let merged = format!("{}{}", t1, t2);
        return clean_time_token(&merged);
    }

    // e.g. "15" "48" -> "15:48"
    if t1.len() == 2
        && t2.len() == 2
        && t1.chars().all(|c| c.is_ascii_digit())
        && t2.chars().all(|c| c.is_ascii_digit())
    {
        let h: u32 = t1.parse().ok()?;
        let m: u32 = t2.parse().ok()?;
        if h < 24 && m < 60 {
            return Some(format!("{:02}:{:02}", h, m));
        }
    }

    None
}

const SPANISH_STOPWORDS: &[&str] = &[
    "de",
    "la",
    "que",
    "el",
    "en",
    "y",
    "a",
    "los",
    "se",
    "del",
    "las",
    "por",
    "un",
    "para",
    "con",
    "no",
    "una",
    "su",
    "al",
    "lo",
    "como",
    "más",
    "pero",
    "sus",
    "le",
    "ya",
    "o",
    "este",
    "sí",
    "porque",
    "esta",
    "entre",
    "cuando",
    "muy",
    "sin",
    "sobre",
    "también",
    "me",
    "hasta",
    "hay",
    "donde",
    "quien",
    "desde",
    "todo",
    "nos",
    "durante",
    "todos",
    "uno",
    "les",
    "ni",
    "contra",
    "otros",
    "ese",
    "eso",
    "ante",
    "ellos",
    "e",
    "esto",
    "mí",
    "antes",
    "algunos",
    "qué",
    "unos",
    "yo",
    "otro",
    "otras",
    "otra",
    "él",
    "tanto",
    "esa",
    "estos",
    "mucho",
    "quienes",
    "nada",
    "muchos",
    "cual",
    "sea",
    "poco",
    "ella",
    "mayor",
    "mucha",
    "fecha",
    "modificación",
    "nombre",
    "tipo",
    "tamaño",
    "aplicación",
    "archivo",
    "carpeta",
    "documento",
    "sistema",
    "configuración",
    "aceptar",
    "cancelar",
    "guardar",
    "cerrar",
    "abrir",
    "ayuda",
    "opciones",
    "información",
    "buscar",
    "kb",
    "mb",
    "gb",
];

const ENGLISH_STOPWORDS: &[&str] = &[
    "the",
    "be",
    "to",
    "of",
    "and",
    "a",
    "in",
    "that",
    "have",
    "i",
    "it",
    "for",
    "not",
    "on",
    "with",
    "he",
    "as",
    "you",
    "do",
    "at",
    "this",
    "but",
    "his",
    "by",
    "from",
    "they",
    "we",
    "say",
    "her",
    "she",
    "or",
    "an",
    "will",
    "my",
    "one",
    "all",
    "would",
    "there",
    "their",
    "what",
    "so",
    "up",
    "out",
    "if",
    "about",
    "who",
    "get",
    "which",
    "go",
    "me",
    "when",
    "make",
    "can",
    "like",
    "time",
    "no",
    "just",
    "him",
    "know",
    "take",
    "person",
    "into",
    "year",
    "your",
    "good",
    "some",
    "could",
    "them",
    "see",
    "other",
    "than",
    "then",
    "now",
    "look",
    "only",
    "come",
    "its",
    "over",
    "think",
    "also",
    "back",
    "after",
    "use",
    "two",
    "how",
    "our",
    "work",
    "first",
    "well",
    "way",
    "even",
    "new",
    "want",
    "because",
    "any",
    "these",
    "give",
    "day",
    "most",
    "us",
    "date",
    "modified",
    "name",
    "type",
    "size",
    "application",
    "file",
    "folder",
    "document",
    "system",
    "settings",
    "cancel",
    "save",
    "close",
    "open",
    "help",
    "kb",
    "mb",
    "gb",
];

fn is_known_alphanumeric_term(token: &str) -> bool {
    let lower = token.to_lowercase();
    let stripped = lower.trim_matches(|c: char| !c.is_alphanumeric());
    matches!(
        stripped,
        "x86"
            | "x64"
            | "win32"
            | "win64"
            | "mp3"
            | "mp4"
            | "h264"
            | "h265"
            | "av1"
            | "utf8"
            | "utf16"
            | "base64"
            | "b64"
            | "sha256"
            | "sha1"
            | "md5"
            | "oauth2"
            | "ipv4"
            | "ipv6"
            | "3d"
            | "4k"
            | "8k"
            | "p2p"
            | "l10n"
            | "i18n"
            | "k8s"
            | "2d"
            | "3g"
            | "4g"
            | "5g"
            | "w3c"
            | "f1"
            | "f2"
            | "f3"
            | "f4"
            | "f5"
            | "f6"
            | "f7"
            | "f8"
            | "f9"
            | "f10"
            | "f11"
            | "f12"
    )
}

fn has_digit_intrusion(token: &str) -> bool {
    let lower = token.to_lowercase();
    let clean = lower.trim_matches(|c: char| !c.is_alphanumeric());
    if is_known_alphanumeric_term(clean) {
        return false;
    }

    let has_letters = clean.chars().any(|c| c.is_alphabetic());
    let has_digits = clean.chars().any(|c| c.is_ascii_digit());
    if !has_letters || !has_digits {
        return false;
    }

    let chars: Vec<char> = clean.chars().collect();
    for i in 0..chars.len() {
        if chars[i].is_ascii_digit() {
            let prev_letter = i > 0 && chars[i - 1].is_alphabetic();
            let next_letter = i + 1 < chars.len() && chars[i + 1].is_alphabetic();
            if prev_letter && (next_letter || i >= 3) {
                return true;
            }
        }
    }
    false
}

fn has_corrupted_symbol(token: &str) -> bool {
    token.chars().any(|c| {
        matches!(
            c,
            '«' | '»' | '•' | '¶' | '§' | '¤' | '¥' | '¢' | '©' | '®' | '™'
        )
    })
}

fn has_unnatural_accents(token: &str, is_spanish: bool) -> bool {
    if is_spanish {
        let lower = token.to_lowercase();
        if lower.contains('ö') || lower.contains('ä') {
            return true;
        }
    }
    false
}

fn count_valid_spanish_accents(s: &str) -> usize {
    s.chars()
        .filter(|c| {
            matches!(
                c,
                'á' | 'é'
                    | 'í'
                    | 'ó'
                    | 'ú'
                    | 'ñ'
                    | 'ü'
                    | 'Á'
                    | 'É'
                    | 'Í'
                    | 'Ó'
                    | 'Ú'
                    | 'Ñ'
                    | 'Ü'
            )
        })
        .count()
}

/// Evaluates an intelligent quality score for recognized text to select the best language output.
pub fn score_recognized_text(text: &RecognizedText) -> f64 {
    let trimmed = text.text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }

    let char_count = trimmed.chars().count() as f64;
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let word_count = words.len() as f64;

    if word_count == 0.0 {
        return 0.0;
    }

    let mut score = 0.0f64;

    // Base score from reasonable character and word count (capped to avoid runaway on hallucinations)
    score += (char_count.min(400.0) * 0.4) + (word_count.min(80.0) * 1.5);

    let lang = text.language_tag.to_lowercase();
    let is_spanish = lang.starts_with("es");
    let is_english = lang.starts_with("en");

    for word in &words {
        let clean = word
            .trim_matches(|c: char| {
                !c.is_alphanumeric()
                    && c != 'ó'
                    && c != 'á'
                    && c != 'é'
                    && c != 'í'
                    && c != 'ú'
                    && c != 'ñ'
                    && c != 'ü'
            })
            .to_lowercase();

        // Check lexicon match
        if (is_spanish && SPANISH_STOPWORDS.contains(&clean.as_str()))
            || (is_english && ENGLISH_STOPWORDS.contains(&clean.as_str()))
        {
            score += 10.0;
        } else if SPANISH_STOPWORDS.contains(&clean.as_str())
            || ENGLISH_STOPWORDS.contains(&clean.as_str())
        {
            score += 4.0;
        }

        // Accents bonus
        let accents = count_valid_spanish_accents(word);
        if accents > 0 {
            score += (accents as f64) * 15.0;
        }

        // Penalty for digit intrusion (e.g. "modificaci6n")
        if has_digit_intrusion(word) {
            score -= 25.0;
        }

        // Penalty for corrupted symbol (e.g. "F«ha")
        if has_corrupted_symbol(word) {
            score -= 30.0;
        }

        // Penalty for unnatural accents in Spanish (e.g. "Aplicaciön", "Tamaöo")
        if has_unnatural_accents(word, is_spanish) {
            score -= 20.0;
        }
    }

    // Reward proportion of alphanumeric/printable characters
    let valid_chars = trimmed
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || c.is_ascii_punctuation()
                || c.is_whitespace()
                || count_valid_spanish_accents(&c.to_string()) > 0
        })
        .count() as f64;
    let validity_ratio = valid_chars / char_count;
    score += validity_ratio * 20.0;

    // Favor structured words of normal length (2 to 25 chars) without corruptions
    let good_words = words
        .iter()
        .filter(|w| {
            (2..=25).contains(&w.chars().count())
                && !has_digit_intrusion(w)
                && !has_corrupted_symbol(w)
        })
        .count() as f64;
    let word_ratio = good_words / word_count;
    score += word_ratio * 25.0;

    score.max(0.0)
}
