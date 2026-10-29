//! Text and regex functions.
#[allow(unused_imports)]
use super::tests::calc;

#[test]
fn regex_functions() {
    let c = [("A1", "Order 123 shipped 2024-05-06")];
    assert_eq!(calc(&c, "=REGEXMATCH(A1,\"\\d+\")"), "TRUE");
    assert_eq!(calc(&c, "=REGEXTEST(A1,\"^order\",1)"), "TRUE");
    assert_eq!(calc(&c, "=REGEXTEST(A1,\"^order\")"), "FALSE");
    assert_eq!(calc(&c, "=REGEXEXTRACT(A1,\"\\d+\")"), "123");
    assert_eq!(calc(&c, "=REGEXEXTRACT(A1,\"(\\d{4})-(\\d\\d)\",2)"), "2024");
    assert_eq!(calc(&c, "=INDEX(REGEXEXTRACT(A1,\"\\d+\",1),3)"), "05");
    assert_eq!(calc(&c, "=REGEXEXTRACT(A1,\"zzz\")"), "#N/A");
    assert_eq!(calc(&c, "=REGEXREPLACE(A1,\"\\d\",\"#\")"), "Order ### shipped ####-##-##");
    assert_eq!(calc(&c, "=REGEXREPLACE(\"a1b2c3\",\"\\d\",\"X\",2)"), "a1bXc3");
    assert_eq!(calc(&c, "=REGEXREPLACE(\"a1b2c3\",\"\\d\",\"X\",-1)"), "a1b2cX");
    assert_eq!(calc(&c, "=REGEXREPLACE(\"john smith\",\"(\\w+) (\\w+)\",\"$2, $1\")"), "smith, john");
    assert_eq!(calc(&c, "=REGEXMATCH(\"aab\",\"^a*a*b$\")"), "TRUE");
    assert_eq!(calc(&c, "=REGEXMATCH(REPT(\"a\",400),\"^a*a*a*a*b$\")"), "#VALUE!");
    assert_eq!(calc(&c, "=REGEXMATCH(REPT(\"a\",28)&\"!\",\"^(.*a){14}$\")"), "#VALUE!");
    assert_eq!(calc(&c, "=REGEXMATCH(\"1.2.3\",\"^(\\d{1,3}\\.){2}\\d{1,3}$\")"), "TRUE");
    assert_eq!(calc(&c, "=REGEXMATCH(\"abcabc\",\"^(abc)\\1$\")"), "TRUE");
    assert_eq!(calc(&c, "=REGEXEXTRACT(\"price: $42\",\"(?<=\\$)\\d+\")"), "42");
    assert_eq!(calc(&c, "=REGEXEXTRACT(\"foobar\",\"foo(?!baz)\\w+?\")"), "foob");
    assert_eq!(calc(&c, "=REGEXMATCH(\"ABC\",\"(?i)abc\")"), "TRUE");
    assert_eq!(calc(&c, "=REGEXMATCH(\"x\",\"(\")"), "#VALUE!");
    assert_eq!(calc(&c, "=REGEXEXTRACT(\"a-b_c\",\"[^-_]+$\")"), "c");
}

#[test]
fn text_extras() {
    let c = [("A1", "1"), ("A2", "x"), ("B1", "TRUE"), ("B2", "")];
    assert_eq!(calc(&c, "=ARRAYTOTEXT(A1:B2)"), "1, TRUE, x, ");
    assert_eq!(calc(&c, "=ARRAYTOTEXT(A1:B2,1)"), "{1,TRUE;\"x\",}");
    assert_eq!(calc(&c, "=VALUETOTEXT(\"hi\",1)"), "\"hi\"");
    assert_eq!(calc(&c, "=VALUETOTEXT(A1,2)"), "#VALUE!");
    assert_eq!(calc(&c, "=ENCODEURL(\"a b&c/é\")"), "a%20b%26c%2F%C3%A9");
    assert_eq!(calc(&c, "=BAHTTEXT(1234.56)"), "หนึ่งพันสองร้อยสามสิบสี่บาทห้าสิบหกสตางค์");
    assert_eq!(calc(&c, "=BAHTTEXT(0)"), "ศูนย์บาทถ้วน");
    assert_eq!(calc(&c, "=BAHTTEXT(1E+307)"), "#NUM!");
    assert_eq!(calc(&c, "=TEXTJOIN(\"|\",1,SPLIT(\"a-b]c^d\\e\",\"]-^\\\"))"), "a|b|c|d|e");
    assert_eq!(calc(&c, "=SUM(SPLIT(\"1,2;3\",\",;\"))"), "6");
    assert_eq!(calc(&c, "=TEXTJOIN(\"|\",1,SPLIT(\"a😀b\",\"😀\",FALSE))"), "a|b");
    assert_eq!(calc(&c, "=JOIN(\"-\",A1:A2,\"z\")"), "1-x-z");
    assert_eq!(calc(&c, "=LEFTB(\"abc\",2)&LENB(\"abc\")&FINDB(\"c\",\"abc\")"), "ab33");
    assert_eq!(calc(&c, "=ASC(\"ＡＢ１\")"), "AB1");
    assert_eq!(calc(&c, "=DBCS(\"AB1\")"), "ＡＢ１");
    assert_eq!(calc(&c, "=ISEMAIL(\"a@b.com\")&ISEMAIL(\"x\")&ISURL(\"http://a.com\")&ISURL(\"x\")&ISURL(\"www.example.org/p\")"), "TRUEFALSETRUEFALSETRUE");
    assert_eq!(calc(&c, "=TO_PURE_NUMBER(\"12\")+TO_PERCENT(1)"), "13");
    assert_eq!(calc(&c, "=TO_TEXT(12)"), "12");
    assert_eq!(calc(&c, "=PHONETIC(A1)"), "#N/A");
}
