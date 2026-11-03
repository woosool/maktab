#[allow(unused_imports)]
use super::tests::calc;

#[test]
fn operators_and_extras() {
    let c = [("A1", "a"), ("A2", "b"), ("A3", "A"), ("A4", ""), ("A5", "1"), ("B1", "1"), ("B2", "2"), ("B3", "3"), ("B5", "5")];
    assert_eq!(calc(&c, "=ADD(2,3)"), "5");
    assert_eq!(calc(&c, "=MINUS(2,3)"), "-1");
    assert_eq!(calc(&c, "=MULTIPLY(2,3)"), "6");
    assert_eq!(calc(&c, "=DIVIDE(1,0)"), "#DIV/0!");
    assert_eq!(calc(&c, "=POW(2,10)"), "1024");
    assert_eq!(calc(&c, "=EQ(\"a\",\"A\")"), "TRUE");
    assert_eq!(calc(&c, "=GT(3,2)"), "TRUE");
    assert_eq!(calc(&c, "=LTE(3,2)"), "FALSE");
    assert_eq!(calc(&c, "=UMINUS(4)"), "-4");
    assert_eq!(calc(&c, "=UNARY_PERCENT(50)"), "0.5");
    assert_eq!(calc(&c, "=ISBETWEEN(5,1,5)"), "TRUE");
    assert_eq!(calc(&c, "=ISBETWEEN(5,1,5,TRUE,FALSE)"), "FALSE");
    assert_eq!(calc(&c, "=EPOCHTODATE(86400)"), "25570");
    assert_eq!(calc(&c, "=EPOCHTODATE(86400000,2)"), "25570");
    assert_eq!(calc(&c, "=EPOCHTODATE(1,4)"), "#VALUE!");
    assert_eq!(calc(&c, "=COUNTUNIQUE(A1:A5)"), "3");
    assert_eq!(calc(&c, "=COUNTUNIQUEIFS(A1:A3,B1:B3,\">1\")"), "2");
    assert_eq!(calc(&c, "=ROWS(FLATTEN(B1:B3,{4,5}))"), "5");
    assert_eq!(calc(&c, "=COLUMNS(ARRAY_CONSTRAIN({1,2,3;4,5,6},1,2))"), "2");
    assert_eq!(calc(&c, "=CELL(\"address\",B2)"), "$B$2");
    assert_eq!(calc(&c, "=CELL(\"row\",B3)"), "3");
    assert_eq!(calc(&c, "=CELL(\"type\",A1)"), "l");
    assert_eq!(calc(&c, "=CELL(\"type\",C9)"), "b");
    assert_eq!(calc(&c, "=CELL(\"contents\",B2)"), "2");
    assert_eq!(calc(&c, "=INFO(\"numfile\")"), "1");
    assert_eq!(calc(&c, "=INFO(\"bogus\")"), "#VALUE!");
    assert_eq!(calc(&c, "=SUM(FILTERXML(\"<r><a>1</a><a>2</a><b><a>3</a></b></r>\",\"//a\"))"), "6");
    assert_eq!(calc(&c, "=FILTERXML(\"<r><a x='7'>1</a></r>\",\"/r/a/@x\")"), "7");
    assert_eq!(calc(&c, "=FILTERXML(\"<r><a>q</a><a>w</a></r>\",\"/r/a[2]\")"), "w");
    assert_eq!(calc(&c, "=FILTERXML(\"<r><a>1</a></r>\",\"//zz\")"), "#N/A");
    assert_eq!(calc(&c, "=FILTERXML(\"<r><a>1</r>\",\"//a\")"), "#VALUE!");
    assert_eq!(calc(&c, "=CUBEVALUE(\"x\")"), "#N/A");
}

#[test]
fn registry_size() { let n = super::names().len(); println!("functions: {n}"); assert!(n >= 550); }
