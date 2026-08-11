use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pinyin,
    Romaji,
    Cantonese,
}

impl Mode {
    pub fn to_string(&self) -> &'static str {
        match self {
            Mode::Pinyin => "pinyin",
            Mode::Romaji => "romaji",
            Mode::Cantonese => "cantonese",
        }
    }
}

pub struct Transcriber {
    phrases: HashMap<String, Vec<String>>,
    words: HashMap<String, Vec<String>>,
    trans: HashMap<String, String>,
}

impl Transcriber {
    pub fn new(dict_dir: &Path) -> Self {
        let mut phrases = HashMap::new();
        let mut words = HashMap::new();
        let mut trans = HashMap::new();

        let mandarin = dict_dir.join("mandarin");
        let cantonese = dict_dir.join("cantonese");

        load_phrases(&mandarin.join("phrases_dict.txt"), &mut phrases);
        load_words(&mandarin.join("word.txt"), &mut words);
        load_trans(&mandarin.join("trans_word.txt"), &mut trans);

        load_phrases(&cantonese.join("phrases_dict.txt"), &mut phrases);
        load_words(&cantonese.join("word.txt"), &mut words);
        load_trans(&cantonese.join("trans_word.txt"), &mut trans);

        Self {
            phrases,
            words,
            trans,
        }
    }

    pub fn transcribe(&self, text: &str, mode: &Mode) -> String {
        match mode {
            Mode::Pinyin => self.transcribe_cn(text, false),
            Mode::Cantonese => self.transcribe_cn(text, true),
            Mode::Romaji => kana_to_romaji(text),
        }
    }

    fn transcribe_cn(&self, text: &str, cantonese: bool) -> String {
        let simplified = self.to_simplified(text);
        let mut result = Vec::new();
        let chars: Vec<char> = simplified.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if is_cjk(chars[i]) {
                let mut matched = false;
                let max_len = (chars.len() - i).min(8);
                for len in (1..=max_len).rev() {
                    let phrase: String = chars[i..i + len].iter().collect();
                    if let Some(syllables) = self.phrases.get(&phrase) {
                        result.extend(syllables.iter().cloned());
                        i += len;
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }
                let ch = chars[i].to_string();
                if let Some(syllables) = self.words.get(&ch) {
                    if let Some(first) = syllables.first() {
                        result.push(first.clone());
                    }
                }
                i += 1;
            } else {
                i += 1;
            }
        }
        result.join(" ")
    }

    fn to_simplified(&self, text: &str) -> String {
        let mut out = String::new();
        for c in text.chars() {
            if let Some(s) = self.trans.get(&c.to_string()) {
                out.push_str(s);
            } else {
                out.push(c);
            }
        }
        out
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF)
}

fn load_phrases(path: &Path, map: &mut HashMap<String, Vec<String>>) {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let syllables: Vec<String> = val
                    .split(',')
                    .map(|s| strip_tone(s.trim()).to_string())
                    .collect();
                if !syllables.is_empty() {
                    map.insert(key.to_string(), syllables);
                }
            }
        }
    }
}

fn load_words(path: &Path, map: &mut HashMap<String, Vec<String>>) {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let syllables: Vec<String> = val
                    .split(',')
                    .map(|s| strip_tone(s.trim()).to_string())
                    .collect();
                if !syllables.is_empty() {
                    map.insert(key.to_string(), syllables);
                }
            }
        }
    }
}

fn load_trans(path: &Path, map: &mut HashMap<String, String>) {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((key, val)) = line.split_once(':') {
                map.insert(key.to_string(), val.trim().to_string());
            }
        }
    }
}

fn strip_tone(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let base = match c {
            'ā' | 'á' | 'ǎ' | 'à' => 'a',
            'ē' | 'é' | 'ě' | 'è' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' => 'u',
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => 'v',
            'Ā' | 'Á' | 'Ǎ' | 'À' => 'A',
            'Ē' | 'É' | 'Ě' | 'È' => 'E',
            'Ī' | 'Í' | 'Ǐ' | 'Ì' => 'I',
            'Ō' | 'Ó' | 'Ǒ' | 'Ò' => 'O',
            'Ū' | 'Ú' | 'Ǔ' | 'Ù' => 'U',
            'Ǖ' | 'Ǘ' | 'Ǚ' | 'Ǜ' | 'Ü' => 'V',
            _ => c,
        };
        out.push(base);
    }
    out
}

fn kana_to_romaji(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let next = chars.peek().copied();
        let romaji = match (c, next) {
            ('き', Some('ゃ')) => { chars.next(); "kya" }
            ('き', Some('ゅ')) => { chars.next(); "kyu" }
            ('き', Some('ょ')) => { chars.next(); "kyo" }
            ('し', Some('ゃ')) => { chars.next(); "sha" }
            ('し', Some('ゅ')) => { chars.next(); "shu" }
            ('し', Some('ょ')) => { chars.next(); "sho" }
            ('ち', Some('ゃ')) => { chars.next(); "cha" }
            ('ち', Some('ゅ')) => { chars.next(); "chu" }
            ('ち', Some('ょ')) => { chars.next(); "cho" }
            ('に', Some('ゃ')) => { chars.next(); "nya" }
            ('に', Some('ゅ')) => { chars.next(); "nyu" }
            ('に', Some('ょ')) => { chars.next(); "nyo" }
            ('ひ', Some('ゃ')) => { chars.next(); "hya" }
            ('ひ', Some('ゅ')) => { chars.next(); "hyu" }
            ('ひ', Some('ょ')) => { chars.next(); "hyo" }
            ('み', Some('ゃ')) => { chars.next(); "mya" }
            ('み', Some('ゅ')) => { chars.next(); "myu" }
            ('み', Some('ょ')) => { chars.next(); "myo" }
            ('り', Some('ゃ')) => { chars.next(); "rya" }
            ('り', Some('ゅ')) => { chars.next(); "ryu" }
            ('り', Some('ょ')) => { chars.next(); "ryo" }
            ('ぎ', Some('ゃ')) => { chars.next(); "gya" }
            ('ぎ', Some('ゅ')) => { chars.next(); "gyu" }
            ('ぎ', Some('ょ')) => { chars.next(); "gyo" }
            ('じ', Some('ゃ')) => { chars.next(); "ja" }
            ('じ', Some('ゅ')) => { chars.next(); "ju" }
            ('じ', Some('ょ')) => { chars.next(); "jo" }
            ('ぢ', Some('ゃ')) => { chars.next(); "ja" }
            ('ぢ', Some('ゅ')) => { chars.next(); "ju" }
            ('ぢ', Some('ょ')) => { chars.next(); "jo" }
            ('び', Some('ゃ')) => { chars.next(); "bya" }
            ('び', Some('ゅ')) => { chars.next(); "byu" }
            ('び', Some('ょ')) => { chars.next(); "byo" }
            ('ぴ', Some('ゃ')) => { chars.next(); "pya" }
            ('ぴ', Some('ゅ')) => { chars.next(); "pyu" }
            ('ぴ', Some('ょ')) => { chars.next(); "pyo" }
            ('っ', Some('k')) => { "k" }
            ('っ', Some('s')) => { "s" }
            ('っ', Some('t')) => { "t" }
            ('っ', Some('p')) => { "p" }
            ('っ', Some('c')) => { "c" }
            ('っ', Some('g')) => { "g" }
            ('っ', Some('d')) => { "d" }
            ('っ', Some('b')) => { "b" }
            ('っ', Some('j')) => { "j" }
            ('っ', Some('z')) => { "z" }
            ('っ', Some('r')) => { "r" }
            ('っ', Some('m')) => { "m" }
            ('っ', Some('n')) => { "n" }
            ('っ', Some('h')) => { "h" }
            ('っ', Some('f')) => { "f" }
            ('っ', Some('w')) => { "w" }
            ('っ', Some('y')) => { "y" }
            ('っ', Some('v')) => { "v" }
            ('っ', _) => "tsu",
            _ => match c {
                'あ' => "a", 'い' => "i", 'う' => "u", 'え' => "e", 'お' => "o",
                'か' => "ka", 'き' => "ki", 'く' => "ku", 'け' => "ke", 'こ' => "ko",
                'さ' => "sa", 'し' => "shi", 'す' => "su", 'せ' => "se", 'そ' => "so",
                'た' => "ta", 'ち' => "chi", 'つ' => "tsu", 'て' => "te", 'と' => "to",
                'な' => "na", 'に' => "ni", 'ぬ' => "nu", 'ね' => "ne", 'の' => "no",
                'は' => "ha", 'ひ' => "hi", 'ふ' => "fu", 'へ' => "he", 'ほ' => "ho",
                'ま' => "ma", 'み' => "mi", 'む' => "mu", 'め' => "me", 'も' => "mo",
                'や' => "ya", 'ゆ' => "yu", 'よ' => "yo",
                'ら' => "ra", 'り' => "ri", 'る' => "ru", 'れ' => "re", 'ろ' => "ro",
                'わ' => "wa", 'を' => "o", 'ん' => "n",
                'が' => "ga", 'ぎ' => "gi", 'ぐ' => "gu", 'げ' => "ge", 'ご' => "go",
                'ざ' => "za", 'じ' => "ji", 'ず' => "zu", 'ぜ' => "ze", 'ぞ' => "zo",
                'だ' => "da", 'ぢ' => "ji", 'づ' => "zu", 'で' => "de", 'ど' => "do",
                'ば' => "ba", 'び' => "bi", 'ぶ' => "bu", 'べ' => "be", 'ぼ' => "bo",
                'ぱ' => "pa", 'ぴ' => "pi", 'ぷ' => "pu", 'ぺ' => "pe", 'ぽ' => "po",
                'ア' => "a", 'イ' => "i", 'ウ' => "u", 'エ' => "e", 'オ' => "o",
                'カ' => "ka", 'キ' => "ki", 'ク' => "ku", 'ケ' => "ke", 'コ' => "ko",
                'サ' => "sa", 'シ' => "shi", 'ス' => "su", 'セ' => "se", 'ソ' => "so",
                'タ' => "ta", 'チ' => "chi", 'ツ' => "tsu", 'テ' => "te", 'ト' => "to",
                'ナ' => "na", 'ニ' => "ni", 'ヌ' => "nu", 'ネ' => "ne", 'ノ' => "no",
                'ハ' => "ha", 'ヒ' => "hi", 'フ' => "fu", 'ヘ' => "he", 'ホ' => "ho",
                'マ' => "ma", 'ミ' => "mi", 'ム' => "mu", 'メ' => "me", 'モ' => "mo",
                'ヤ' => "ya", 'ユ' => "yu", 'ヨ' => "yo",
                'ラ' => "ra", 'リ' => "ri", 'ル' => "ru", 'レ' => "re", 'ロ' => "ro",
                'ワ' => "wa", 'ヲ' => "o", 'ン' => "n",
                'ガ' => "ga", 'ギ' => "gi", 'グ' => "gu", 'ゲ' => "ge", 'ゴ' => "go",
                'ザ' => "za", 'ジ' => "ji", 'ズ' => "zu", 'ゼ' => "ze", 'ゾ' => "zo",
                'ダ' => "da", 'ヂ' => "ji", 'ヅ' => "zu", 'デ' => "de", 'ド' => "do",
                'バ' => "ba", 'ビ' => "bi", 'ブ' => "bu", 'ベ' => "be", 'ボ' => "bo",
                'パ' => "pa", 'ピ' => "pi", 'プ' => "pu", 'ペ' => "pe", 'ポ' => "po",
                'ー' => "-",
                _ => {
                    if c.is_whitespace() {
                        " "
                    } else {
                        continue;
                    }
                }
            },
        };
        out.push_str(romaji);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
