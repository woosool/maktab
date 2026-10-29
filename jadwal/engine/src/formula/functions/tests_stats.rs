//! Statistics checked against Excel's results.
#[allow(unused_imports)]
use super::tests::calc;

/// The formula's result rounded to `d` decimals (as text, for comparison).
fn r(cells: &[(&str, &str)], f: &str, d: usize) -> String {
    let v = calc(cells, f);
    match v.parse::<f64>() { Ok(x) => format!("{:.*}", d, x), Err(_) => v }
}

#[test]
fn distributions() {
    let e: [(&str, &str); 0] = [];
    assert_eq!(r(&e, "=NORM.DIST(42,40,1.5,TRUE)", 9), "0.908788780");
    assert_eq!(r(&e, "=NORM.DIST(42,40,1.5,FALSE)", 9), "0.109340050");
    assert_eq!(r(&e, "=NORM.S.DIST(1.333333,TRUE)", 9), "0.908788726");
    assert_eq!(r(&e, "=NORM.INV(0.908789,40,1.5)", 6), "42.000002");
    assert_eq!(r(&e, "=NORM.S.INV(0.908789)", 7), "1.3333347");
    assert_eq!(r(&e, "=LOGNORM.DIST(4,3.5,1.2,TRUE)", 7), "0.0390836");
    assert_eq!(r(&e, "=LOGNORM.INV(0.039084,3.5,1.2)", 5), "4.00003");
    assert_eq!(r(&e, "=EXPON.DIST(0.2,10,TRUE)", 9), "0.864664717");
    assert_eq!(r(&e, "=POISSON.DIST(2,5,TRUE)", 9), "0.124652019");
    assert_eq!(r(&e, "=POISSON.DIST(2,5,FALSE)", 9), "0.084224337");
    assert_eq!(r(&e, "=BINOM.DIST(6,10,0.5,FALSE)", 9), "0.205078125");
    assert_eq!(r(&e, "=BINOM.DIST(6,10,0.5,TRUE)", 9), "0.828125000");
    assert_eq!(r(&e, "=BINOM.DIST.RANGE(60,0.75,45,50)", 7), "0.5236298");
    assert_eq!(calc(&e, "=BINOM.INV(6,0.5,0.75)"), "4");
    assert_eq!(r(&e, "=WEIBULL.DIST(105,20,100,TRUE)", 9), "0.929581390");
    assert_eq!(r(&e, "=GAMMA.DIST(10.00001131,9,2,TRUE)", 7), "0.0680940");
    assert_eq!(r(&e, "=GAMMA.INV(0.068094,9,2)", 4), "10.0000");
    assert_eq!(r(&e, "=GAMMA(2.5)", 9), "1.329340388");
    assert_eq!(r(&e, "=GAMMALN(4.5)", 9), "2.453736571");
    assert_eq!(calc(&e, "=GAMMA(0)"), "#NUM!");
    assert_eq!(r(&e, "=CHISQ.DIST(0.5,1,TRUE)", 9), "0.520499878");
    assert_eq!(r(&e, "=CHISQ.DIST.RT(18.307,10)", 7), "0.0500006");
    assert_eq!(r(&e, "=CHISQ.INV(0.93,1)", 7), "3.2830203");
    assert_eq!(r(&e, "=CHISQ.INV.RT(0.05,10)", 6), "18.307038");
    assert_eq!(r(&e, "=T.DIST(60,1,TRUE)", 9), "0.994695326");
    assert_eq!(r(&e, "=T.DIST.2T(1.959999998,60)", 8), "0.05464493");
    assert_eq!(r(&e, "=T.DIST.RT(1.959999998,60)", 8), "0.02732246");
    assert_eq!(r(&e, "=T.INV(0.75,2)", 9), "0.816496581");
    assert_eq!(r(&e, "=T.INV.2T(0.546449,60)", 6), "0.606533");
    assert_eq!(r(&e, "=F.DIST(15.2069,6,4,TRUE)", 7), "0.9900000");
    assert_eq!(r(&e, "=F.DIST.RT(15.2068649,6,4)", 7), "0.0100000");
    assert_eq!(r(&e, "=F.INV(0.01,6,4)", 7), "0.1093099");
    assert_eq!(r(&e, "=F.INV.RT(0.01,6,4)", 5), "15.20686");
    assert_eq!(r(&e, "=BETA.DIST(2,8,10,TRUE,1,3)", 9), "0.685470581");
    assert_eq!(r(&e, "=BETA.DIST(2,8,10,FALSE,1,3)", 7), "1.4837646");
    assert_eq!(r(&e, "=BETA.INV(0.685470581,8,10,1,3)", 6), "2.000000");
    assert_eq!(r(&e, "=HYPGEOM.DIST(1,4,8,20,TRUE)", 7), "0.4654283");
    assert_eq!(r(&e, "=HYPGEOM.DIST(1,4,8,20,FALSE)", 7), "0.3632611");
    assert_eq!(r(&e, "=NEGBINOM.DIST(10,5,0.25,TRUE)", 7), "0.3135141");
    assert_eq!(r(&e, "=NEGBINOM.DIST(10,5,0.25,FALSE)", 7), "0.0550487");
    assert_eq!(r(&e, "=CONFIDENCE.NORM(0.05,2.5,50)", 7), "0.6929519");
    assert_eq!(r(&e, "=CONFIDENCE.T(0.05,1,50)", 7), "0.2841969");
    assert_eq!(r(&e, "=FISHER(0.75)", 9), "0.972955075");
    assert_eq!(r(&e, "=FISHERINV(0.972955)", 6), "0.750000");
    assert_eq!(r(&e, "=STANDARDIZE(42,40,1.5)", 9), "1.333333333");
    assert_eq!(r(&e, "=GAUSS(2)", 9), "0.477249868");
    assert_eq!(r(&e, "=PHI(0.75)", 9), "0.301137432");
    assert_eq!(calc(&e, "=NORM.INV(0,0,1)"), "#NUM!");
}

#[test]
fn descriptive_and_ranks() {
    let c = [("A1", "3"), ("A2", "4"), ("A3", "5"), ("A4", "2"), ("A5", "3"), ("A6", "4"), ("A7", "5"), ("A8", "6"), ("A9", "4"), ("A10", "7")];
    assert_eq!(r(&c, "=SKEW(A1:A10)", 9), "0.359543071");
    assert_eq!(r(&c, "=SKEW.P(A1:A10)", 9), "0.303193339");
    assert_eq!(r(&c, "=KURT(A1:A10)", 9), "-0.151799637");
    assert_eq!(r(&c, "=DEVSQ(A1:A7)", 6), "7.428571");
    assert_eq!(r(&c, "=AVEDEV(A1:A7)", 9), "0.897959184");
    assert_eq!(r(&c, "=GEOMEAN(A1:A7)", 9), "3.556702167");
    assert_eq!(r(&c, "=HARMEAN(A1:A7)", 9), "3.387096774");
    assert_eq!(r(&c, "=PERCENTILE(A1:A10,0.3)", 6), "3.700000");
    assert_eq!(r(&c, "=PERCENTILE.EXC(A1:A10,0.3)", 6), "3.300000");
    assert_eq!(calc(&c, "=QUARTILE(A1:A10,1)"), "3.25");
    assert_eq!(calc(&c, "=QUARTILE.EXC(A1:A10,1)"), "3");
    assert_eq!(calc(&c, "=PERCENTRANK(A1:A10,4)"), "0.333");
    assert_eq!(calc(&c, "=PERCENTRANK.EXC(A1:A10,4)"), "0.363");
    assert_eq!(calc(&c, "=RANK.AVG(4,A1:A10)"), "6");
    assert_eq!(calc(&c, "=TRIMMEAN(A1:A10,0.2)"), "4.25");
    assert_eq!(calc(&c, "=INDEX(MODE.MULT(A1:A10),1)"), "4");
    assert_eq!(calc(&c, "=INDEX(FREQUENCY(A1:A10,{3;5}),3)"), "2");
    assert_eq!(r(&c, "=STDEVA(A1:A10,TRUE)", 6), "1.732051");
    assert_eq!(r(&c, "=VARPA(A1:A10)", 4), "2.0100");
    assert_eq!(r(&c, "=AGGREGATE(14,6,A1:A10,2)", 4), "6.0000");
    assert_eq!(calc(&c, "=AGGREGATE(9,0,A1:A10)"), "43");
    assert_eq!(r(&c, "=PROB({1;2;3;4},{0.1;0.2;0.3;0.4},2,3)", 6), "0.500000");
    assert_eq!(r(&c, "=MARGINOFERROR(A1:A10,0.95)", 6), "0.926243");
}

#[test]
fn pairs_and_regression() {
    let c = [("A1", "1"), ("A2", "2"), ("A3", "3"), ("A4", "4"), ("A5", "5"), ("B1", "2"), ("B2", "4"), ("B3", "5"), ("B4", "4"), ("B5", "5")];
    assert_eq!(r(&c, "=CORREL(A1:A5,B1:B5)", 9), "0.774596669");
    assert_eq!(r(&c, "=PEARSON(A1:A5,B1:B5)", 9), "0.774596669");
    assert_eq!(r(&c, "=RSQ(B1:B5,A1:A5)", 6), "0.600000");
    assert_eq!(r(&c, "=SLOPE(B1:B5,A1:A5)", 6), "0.600000");
    assert_eq!(r(&c, "=INTERCEPT(B1:B5,A1:A5)", 6), "2.200000");
    assert_eq!(r(&c, "=STEYX(B1:B5,A1:A5)", 9), "0.894427191");
    assert_eq!(r(&c, "=FORECAST(6,B1:B5,A1:A5)", 6), "5.800000");
    assert_eq!(r(&c, "=COVARIANCE.P(A1:A5,B1:B5)", 6), "1.200000");
    assert_eq!(r(&c, "=COVARIANCE.S(A1:A5,B1:B5)", 6), "1.500000");
    assert_eq!(r(&c, "=INDEX(LINEST(B1:B5,A1:A5),1)", 6), "0.600000");
    assert_eq!(r(&c, "=INDEX(LINEST(B1:B5,A1:A5),2)", 6), "2.200000");
    assert_eq!(r(&c, "=INDEX(LINEST(B1:B5,A1:A5,TRUE,TRUE),3,1)", 6), "0.600000");
    assert_eq!(r(&c, "=INDEX(LINEST(B1:B5,A1:A5,TRUE,TRUE),4,2)", 6), "3.000000");
    assert_eq!(r(&c, "=INDEX(TREND(B1:B5,A1:A5,{6;7}),2)", 6), "6.400000");
    assert_eq!(r(&c, "=INDEX(GROWTH(B1:B5,A1:A5,{6}),1)", 6), "6.597540");
    assert_eq!(r(&c, "=INDEX(LOGEST(B1:B5,A1:A5),1)", 6), "1.201124");
    assert_eq!(r(&c, "=CHISQ.TEST(A1:A5,B1:B5)", 6), "0.680769");
    assert_eq!(r(&c, "=T.TEST(A1:A5,B1:B5,2,1)", 6), "0.089009");
    assert_eq!(r(&c, "=T.TEST(A1:A5,B1:B5,2,2)", 6), "0.295999");
    assert_eq!(r(&c, "=T.TEST(A1:A5,B1:B5,2,3)", 6), "0.297952");
    assert_eq!(r(&c, "=F.TEST(A1:A5,B1:B5)", 6), "0.632812");
    assert_eq!(r(&c, "=Z.TEST(A1:A5,2)", 6), "0.078650");
}

#[test]
fn ets() {
    let mut c: Vec<(String, String)> = vec![];
    for i in 1..=12 { c.push((format!("A{i}"), i.to_string())); c.push((format!("B{i}"), (10 + 2 * i).to_string())); }
    let cells: Vec<(&str, &str)> = c.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    assert_eq!(r(&cells, "=FORECAST.ETS(13,B1:B12,A1:A12)", 6), "36.000000");
    assert_eq!(calc(&cells, "=FORECAST.ETS.SEASONALITY(B1:B12,A1:A12)"), "0");
    assert_eq!(calc(&cells, "=FORECAST.ETS.STAT(B1:B12,A1:A12,1)"), "0.9");
    assert_eq!(calc(&cells, "=FORECAST.ETS.STAT(B1:B12,A1:A12,8)"), "1");
    assert_eq!(r(&cells, "=FORECAST.ETS.CONFINT(13,B1:B12,A1:A12)", 6), "0.000000");
}
