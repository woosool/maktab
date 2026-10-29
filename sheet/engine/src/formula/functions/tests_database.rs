#[allow(unused_imports)]
use super::tests::calc;

#[test]
fn database_functions() {
    let c = [
        ("A1", "Tree"), ("B1", "Height"), ("C1", "Age"), ("D1", "Yield"), ("E1", "Profit"),
        ("A2", "Apple"), ("B2", "18"), ("C2", "20"), ("D2", "14"), ("E2", "105"),
        ("A3", "Pear"), ("B3", "12"), ("C3", "12"), ("D3", "10"), ("E3", "96"),
        ("A4", "Cherry"), ("B4", "13"), ("C4", "14"), ("D4", "9"), ("E4", "105"),
        ("A5", "Apple"), ("B5", "14"), ("C5", "15"), ("D5", "10"), ("E5", "75"),
        ("A6", "Pear"), ("B6", "9"), ("C6", "8"), ("D6", "8"), ("E6", "76.8"),
        ("A7", "Apple"), ("B7", "8"), ("C7", "9"), ("D7", "6"), ("E7", "45"),
        ("G1", "Tree"), ("H1", "Height"), ("I1", "Height"), ("G2", "=\"=Apple\""), ("H2", ">10"), ("I2", "<16"), ("G3", "=\"=Pear\""),
    ];
    assert_eq!(calc(&c, "=DCOUNT(A1:E7,\"Age\",G1:I2)"), "1");
    assert_eq!(calc(&c, "=DCOUNTA(A1:E7,\"Profit\",G1:I2)"), "1");
    assert_eq!(calc(&c, "=DMAX(A1:E7,\"Profit\",G1:I3)"), "96");
    assert_eq!(calc(&c, "=DMIN(A1:E7,\"Profit\",G1:I2)"), "75");
    assert_eq!(calc(&c, "=DAVERAGE(A1:E7,\"Yield\",G1:H2)"), "12");
    assert_eq!(calc(&c, "=DSUM(A1:E7,\"Profit\",G1:G2)"), "225");
    assert_eq!(calc(&c, "=DSUM(A1:E7,5,G1:I2)"), "75");
    assert_eq!(calc(&c, "=DPRODUCT(A1:E7,\"Yield\",G1:I2)"), "10");
    assert_eq!(calc(&c, "=ROUND(DSTDEV(A1:E7,\"Yield\",G1:G3),5)"), "2.96648");
    assert_eq!(calc(&c, "=ROUND(DSTDEVP(A1:E7,\"Yield\",G1:G3),5)"), "2.6533");
    assert_eq!(calc(&c, "=ROUND(DVAR(A1:E7,\"Yield\",G1:G3),1)"), "8.8");
    assert_eq!(calc(&c, "=ROUND(DVARP(A1:E7,\"Yield\",G1:G3),2)"), "7.04");
    assert_eq!(calc(&c, "=DGET(A1:E7,\"Yield\",G1:I2)"), "10");
    assert_eq!(calc(&c, "=DGET(A1:E7,\"Yield\",G1:G3)"), "#NUM!");
    assert_eq!(calc(&c, "=DSUM(A1:E7,\"Nope\",G1:G2)"), "#VALUE!");
}
