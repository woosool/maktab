#[allow(unused_imports)]
use super::tests::calc;

#[test]
fn grouping() {
    let c = [("A1", "x"), ("A2", "y"), ("A3", "x"), ("A4", "z"), ("B1", "1"), ("B2", "2"), ("B3", "3"), ("B4", "4"),
             ("C1", "p"), ("C2", "q"), ("C3", "q"), ("C4", "p")];
    assert_eq!(calc(&c, "=ROWS(GROUPBY(A1:A4,B1:B4,SUM))"), "4");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,SUM),1,2)"), "4");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,SUM),4,1)"), "Total");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,SUM),4,2)"), "10");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,SUM,,0,-2),1,1)"), "x");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,LAMBDA(v,MAX(v)),,0),1,2)"), "3");
    assert_eq!(calc(&c, "=INDEX(GROUPBY(A1:A4,B1:B4,PERCENTOF,,0),1,2)"), "0.4");
    assert_eq!(calc(&c, "=GROUPBY(A1:A4,B1:B4,1)"), "#VALUE!");
    assert_eq!(calc(&c, "=INDEX(PIVOTBY(A1:A4,C1:C4,B1:B4,SUM),1,2)"), "p");
    assert_eq!(calc(&c, "=INDEX(PIVOTBY(A1:A4,C1:C4,B1:B4,SUM),2,3)"), "3");
    assert_eq!(calc(&c, "=INDEX(PIVOTBY(A1:A4,C1:C4,B1:B4,SUM),5,4)"), "10");
    assert_eq!(calc(&c, "=PERCENTOF(B1:B2,B1:B4)"), "0.3");
    assert_eq!(calc(&[("A2", "1"), ("B3", "2")], "=ROWS(TRIMRANGE(A1:D9))"), "2");
    assert_eq!(calc(&[("A2", "1"), ("B3", "2")], "=SUM(TRIMRANGE(A1:D9))"), "3");
    assert_eq!(calc(&c, "=SINGLE({7,8})"), "7");
}
