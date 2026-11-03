#[allow(unused_imports)]
use super::tests::calc;

fn c(f: &str) -> String { calc(&[], f) }

#[test]
fn bases_and_bits() {
    assert_eq!(c("=DEC2BIN(9,8)"), "00001001");
    assert_eq!(c("=DEC2BIN(-1)"), "1111111111");
    assert_eq!(c("=DEC2HEX(255)"), "FF");
    assert_eq!(c("=DEC2HEX(-1)"), "FFFFFFFFFF");
    assert_eq!(c("=HEX2DEC(\"FFFFFFFFFF\")"), "-1");
    assert_eq!(c("=BIN2DEC(1100100)"), "100");
    assert_eq!(c("=OCT2HEX(\"7777777777\")"), "FFFFFFFFFF");
    assert_eq!(c("=HEX2BIN(\"F\",8)"), "00001111");
    assert_eq!(c("=DEC2BIN(512)"), "#NUM!");
    assert_eq!(c("=BASE(255,16,4)"), "00FF");
    assert_eq!(c("=DECIMAL(\"zap\",36)"), "45745");
    assert_eq!(c("=BITAND(13,25)"), "9");
    assert_eq!(c("=BITXOR(5,3)"), "6");
    assert_eq!(c("=BITLSHIFT(4,2)"), "16");
    assert_eq!(c("=BITRSHIFT(13,2)"), "3");
    assert_eq!(c("=DELTA(5,5)"), "1");
    assert_eq!(c("=GESTEP(-1)"), "0");
}

#[test]
fn roman_convert_erf() {
    assert_eq!(c("=ROMAN(499)"), "CDXCIX");
    assert_eq!(c("=ROMAN(499,4)"), "ID");
    assert_eq!(c("=ROMAN(1999,1)"), "MLMVLIV");
    assert_eq!(c("=ARABIC(\"MCMXCIX\")"), "1999");
    assert_eq!(c("=CONVERT(1,\"lbm\",\"kg\")"), "0.45359237");
    assert_eq!(c("=CONVERT(68,\"F\",\"C\")"), "20");
    assert_eq!(c("=CONVERT(1,\"km\",\"m\")"), "1000");
    assert_eq!(c("=CONVERT(1,\"m^2\",\"cm2\")"), "10000");
    assert_eq!(c("=CONVERT(1,\"kibyte\",\"bit\")"), "8192");
    assert_eq!(c("=CONVERT(1,\"m\",\"g\")"), "#N/A");
    assert_eq!(c("=ROUND(ERF(1),9)"), "0.842700793");
    assert_eq!(c("=ROUND(ERFC(1),9)"), "0.157299207");
    assert_eq!(c("=ROUND(ERF(0,1.5),9)"), "0.966105146");
}

#[test]
fn bessel() {
    assert_eq!(c("=ROUND(BESSELJ(1.9,2),9)"), "0.329925728");
    assert_eq!(c("=ROUND(BESSELI(1.5,1),8)"), "0.98166643");
    assert_eq!(c("=ROUND(BESSELK(1.5,1),8)"), "0.2773878");
    assert_eq!(c("=ROUND(BESSELY(2.5,1),8)"), "0.14591814");
    assert_eq!(c("=BESSELY(-1,1)"), "#NUM!");
}

#[test]
fn complex() {
    assert_eq!(c("=COMPLEX(3,4)"), "3+4i");
    assert_eq!(c("=COMPLEX(0,1.5E-20)"), "1.5E-20i");
    assert_eq!(c("=COMPLEX(1E10,0.00001)"), "10000000000+0.00001i");
    assert_eq!(c("=COMPLEX(0,-1,\"j\")"), "-j");
    assert_eq!(c("=IMABS(\"3+4i\")"), "5");
    assert_eq!(c("=IMREAL(\"6-9i\")"), "6");
    assert_eq!(c("=IMAGINARY(\"-j\")"), "-1");
    assert_eq!(c("=IMSUM(\"3+4i\",\"5-3i\")"), "8+i");
    assert_eq!(c("=IMSUB(\"13+4i\",\"5+3i\")"), "8+i");
    assert_eq!(c("=IMPRODUCT(\"3+4i\",\"5-3i\")"), "27+11i");
    assert_eq!(c("=IMDIV(\"-238+240i\",\"10+24i\")"), "5+12i");
    assert_eq!(c("=IMPOWER(\"2+3i\",3)"), "-46+9i");
    assert_eq!(c("=IMPOWER(\"i\",2)"), "-1");
    assert_eq!(c("=IMSQRT(\"-4\")"), "2i");
    assert_eq!(c("=IMEXP(\"0\")"), "1");
    assert_eq!(c("=IMLN(\"3+4i\")"), "1.6094379124341+0.927295218001612i");
    assert_eq!(c("=IMCONJUGATE(\"3+4i\")"), "3-4i");
    assert_eq!(c("=IMSIN(\"4+3i\")"), "-7.61923172032141-6.548120040911i");
    assert_eq!(c("=IMSUM(\"1+i\",\"1+j\")"), "#VALUE!");
    assert_eq!(c("=IMABS(\"x\")"), "#NUM!");
}
