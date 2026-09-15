//! Conservative cleanup of spacing introduced by word-level Chinese ASR.
fn han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x323af)
}
fn chinese_punctuation(c: char) -> bool {
    "，。！？；：、（）【】《》〈〉「」『』".contains(c)
}

pub(crate) fn readable_spacing(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut previous = None;
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if let Some(left) = previous
            && space
            && !(han(left) && han(c) || chinese_punctuation(left) || chinese_punctuation(c))
        {
            result.push(' ');
        }
        result.push(c);
        previous = Some(c);
        space = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repairs_chinese_without_joining_english_or_korean_words() {
        assert_eq!(
            readable_spacing("末 日 爆 发 前 夕， 我 收 到 导 师 的 警 告 短 信"),
            "末日爆发前夕，我收到导师的警告短信"
        );
        assert_eq!(
            readable_spacing("使 用 OpenAI API 和 5000 瓦 发 电 机"),
            "使用 OpenAI API 和 5000 瓦发电机"
        );
        assert_eq!(readable_spacing("  hello   world ! "), "hello world !");
        assert_eq!(readable_spacing("한국어 단어 사이"), "한국어 단어 사이");
        assert_eq!(readable_spacing("𠀀 𠀁 中 文"), "𠀀𠀁中文");
    }
}
