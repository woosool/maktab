//! Formula results checked against Excel's.

use crate::addr::parse_a1;
use crate::book::Workbook;

/// A workbook with `cells` entered (A1-style address, typed text), recalculated.
pub fn book(cells: &[(&str, &str)]) -> Workbook {
    let mut wb = Workbook::new();
    for (a, t) in cells { wb.set_input(0, parse_a1(a).unwrap(), t); }
    wb.recalc();
    wb
}

/// The displayed result of a formula typed into Z1000 of a book with `cells`.
pub fn calc(cells: &[(&str, &str)], formula: &str) -> String {
    let mut all = cells.to_vec();
    all.push(("Z1000", formula));
    let wb = book(&all);
    wb.sheets[0].value(parse_a1("Z1000").unwrap()).display()
}

#[test]
fn basics() {
    let c = [("A1", "1"), ("A2", "2"), ("A3", "3"), ("B1", "x"), ("B2", "y"), ("B3", "x")];
    assert_eq!(calc(&c, "=SUM(A1:A3)"), "6");
    assert_eq!(calc(&c, "=AVERAGE(A1:A3)*2"), "4");
    assert_eq!(calc(&c, "=SUMIF(B1:B3,\"x\",A1:A3)"), "4");
    assert_eq!(calc(&c, "=COUNTIFS(B1:B3,\"x\",A1:A3,\">1\")"), "1");
    assert_eq!(calc(&c, "=VLOOKUP(2,A1:B3,2,FALSE)"), "y");
    assert_eq!(calc(&c, "=XLOOKUP(\"y\",B1:B3,A1:A3)"), "2");
    assert_eq!(calc(&c, "=INDEX(A1:A3,MATCH(3,A1:A3,0))"), "3");
    assert_eq!(calc(&c, "=IF(A1>0,\"pos\",\"neg\")"), "pos");
    assert_eq!(calc(&c, "=IFERROR(1/0,\"div\")"), "div");
    assert_eq!(calc(&c, "=-2^2"), "4");
    assert_eq!(calc(&c, "=\"a\"&1&TRUE"), "a1TRUE");
    assert_eq!(calc(&c, "=1/0"), "#DIV/0!");
    assert_eq!(calc(&c, "=LET(x,2,y,3,x*y)"), "6");
    assert_eq!(calc(&c, "=LAMBDA(x,x+1)(4)"), "5");
    assert_eq!(calc(&c, "=SUMPRODUCT(A1:A3,A1:A3)"), "14");
    assert_eq!(calc(&c, "=TEXT(0.256,\"0.0%\")"), "25.6%");
    assert_eq!(calc(&c, "=DATE(2024,1,15)"), "45306");
    assert_eq!(calc(&c, "=ROUND(2.345,2)"), "2.35");
    assert_eq!(calc(&c, "=SUM(SEQUENCE(4))"), "10");
    assert_eq!(calc(&c, "=ROWS(FILTER(A1:A3,A1:A3>1))"), "2");
    assert_eq!(calc(&c, "=SUM(OFFSET(A1,1,0,2,1))"), "5");
    assert_eq!(calc(&c, "=SUM(INDIRECT(\"A1:A2\"))"), "3");
    assert_eq!(calc(&c, "=TEXTJOIN(\"-\",TRUE,B1:B3)"), "x-y-x");
    assert_eq!(calc(&c, "=REDUCE(0,A1:A3,LAMBDA(a,b,a+b))"), "6");
    assert_eq!(calc(&c, "=BYROW(A1:A3,SUM)"), "1");
}

#[test]
fn spill_and_cycles() {
    let wb = book(&[("A1", "=SEQUENCE(3)"), ("B1", "=SUM(A1#)"), ("C1", "=C1+1"), ("D2", "=A3*10")]);
    assert_eq!(wb.display(0, parse_a1("A3").unwrap()), "3");
    assert_eq!(wb.display(0, parse_a1("B1").unwrap()), "6");
    assert_eq!(wb.display(0, parse_a1("D2").unwrap()), "30");
    assert_eq!(wb.display(0, parse_a1("C1").unwrap()), "#CYCLE!");
    let wb = book(&[("A1", "=SEQUENCE(3)"), ("A2", "x")]);
    assert_eq!(wb.display(0, parse_a1("A1").unwrap()), "#SPILL!");
}

#[test]
fn chain() {
    // A long chain must not overflow the stack.
    let mut wb = Workbook::new();
    wb.set_input(0, parse_a1("A1").unwrap(), "1");
    for r in 2..=20000 { wb.set_input(0, crate::Pos::new(r - 1, 0), &format!("=A{}+1", r - 1)); }
    // Enter in reverse order of dependency too.
    wb.set_input(0, parse_a1("B20000").unwrap(), "=A20000");
    wb.recalc();
    assert_eq!(wb.display(0, parse_a1("B20000").unwrap()), "20000");
}

#[test]
fn editing() {
    let mut wb = book(&[("A1", "1"), ("A2", "2"), ("A3", "=SUM(A1:A2)")]);
    wb.insert(0, true, 1, 1);
    assert_eq!(wb.edit_text(0, parse_a1("A4").unwrap()), "=SUM(A1:A3)");
    wb.insert(0, true, 0, -1);
    assert_eq!(wb.edit_text(0, parse_a1("A3").unwrap()), "=SUM(#REF!)".replace("#REF!", "A1:A2"));
    let mut wb = book(&[("A1", "Mon"), ("B1", "1"), ("B2", "3"), ("C1", "Item 1")]);
    wb.fill(0, crate::addr::parse_range("A1").unwrap(), crate::addr::parse_range("A1:A3").unwrap(), true);
    wb.fill(0, crate::addr::parse_range("B1:B2").unwrap(), crate::addr::parse_range("B1:B4").unwrap(), true);
    wb.fill(0, crate::addr::parse_range("C1").unwrap(), crate::addr::parse_range("C1:C3").unwrap(), true);
    assert_eq!(wb.display(0, parse_a1("A3").unwrap()), "Wed");
    assert_eq!(wb.display(0, parse_a1("B4").unwrap()), "7");
    assert_eq!(wb.display(0, parse_a1("C3").unwrap()), "Item 3");
}
