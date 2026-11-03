#[allow(unused_imports)]
use super::tests::calc;

#[test]
fn matrices_and_math() {
    let c = [("A1", "1"), ("B1", "2"), ("A2", "3"), ("B2", "4"), ("C1", "5"), ("C2", "6")];
    assert_eq!(calc(&c, "=MDETERM(A1:B2)"), "-2");
    assert_eq!(calc(&c, "=INDEX(MINVERSE(A1:B2),1,1)"), "-2");
    assert_eq!(calc(&c, "=INDEX(MINVERSE(A1:B2),2,1)"), "1.5");
    assert_eq!(calc(&c, "=MINVERSE({1,2;2,4})"), "#NUM!");
    assert_eq!(calc(&c, "=MINVERSE(A1:C2)"), "#VALUE!");
    assert_eq!(calc(&c, "=INDEX(MMULT(A1:B2,A1:B2),2,2)"), "22");
    assert_eq!(calc(&c, "=MMULT(A1:C2,A1:B2)"), "#VALUE!");
    assert_eq!(calc(&c, "=SUM(MUNIT(3))"), "3");
    assert_eq!(calc(&c, "=SUMX2MY2(A1:A2,B1:B2)"), "-10");
    assert_eq!(calc(&c, "=SUMX2PY2(A1:A2,B1:B2)"), "30");
    assert_eq!(calc(&c, "=SUMXMY2(A1:A2,B1:B2)"), "2");
    assert_eq!(calc(&c, "=SERIESSUM(2,1,1,{1,2,3})"), "34");
    assert_eq!(calc(&c, "=MULTINOMIAL(2,3,4)"), "1260");
    assert_eq!(calc(&c, "=COMBINA(4,3)"), "20");
    assert_eq!(calc(&c, "=PERMUTATIONA(3,2)"), "9");
    assert_eq!(calc(&c, "=FACTDOUBLE(6)"), "48");
    assert_eq!(calc(&c, "=FACTDOUBLE(7)"), "105");
    assert_eq!(calc(&c, "=ISO.CEILING(-4.3,2)"), "-4");
    assert_eq!(calc(&c, "=ROUND(ACOSH(4),6)"), "2.063437");
    assert_eq!(calc(&c, "=ROUND(COT(30),6)"), "-0.15612");
    assert_eq!(calc(&c, "=COT(0)"), "#DIV/0!");
    assert_eq!(calc(&c, "=ATANH(1)"), "#NUM!");
    assert_eq!(calc(&c, "=ROUND(ACOTH(6),6)"), "0.168236");
}
