//! Starting points for File ▸ New (core/templates.js): small, complete workbooks with live
//! formulas and formats already in place. Blue cells are the ones meant to be edited.

use crate::addr::{parse_a1, Pos, Range};
use crate::style::{HAlign, Style};
use crate::Workbook;

const INPUT: [u8; 3] = [0x0a, 0x63, 0xf0];
const HEAD_BG: [u8; 3] = [0xee, 0xf2, 0xff];
const HEAD_FG: [u8; 3] = [0x1e, 0x3a, 0x8a];
const MUTED: [u8; 3] = [0x6b, 0x69, 0x64];
const MONEY: &str = "\"$\"#,##0.00";

/// (id, name, description, icon) of each template, in gallery order.
pub const TEMPLATES: [(&str, &str, &str, &str); 7] = [
    ("blank", "Blank workbook", "An empty grid.", "filePlus"),
    ("invoice", "Invoice", "Line items with quantities, prices, tax, and the total due.", "sheet"),
    ("todo", "To-do list", "Tasks with due dates, priorities, and counts of what is open and overdue.", "accept"),
    ("loan", "Loan calculator", "Monthly payment, total interest, and a 60-payment amortization schedule.", "percentStyle"),
    ("gradebook", "Grade book", "Weighted averages, letter grades, and class statistics.", "table"),
    ("schedule", "Weekly schedule", "Hours down the side, days across the top, dated from this Monday.", "dateTime"),
    ("habits", "Habit tracker", "Habits down the side, a mark per day, and a completion rate for the month.", "accept"),
];

#[derive(Default, Clone, Copy)]
struct Fmt { bold: bool, size: u32, color: Option<[u8; 3]>, fill: Option<[u8; 3]>, h: Option<HAlign>, num: Option<&'static str> }
const PLAIN: Fmt = Fmt { bold: false, size: 0, color: None, fill: None, h: None, num: None };
const HEAD: Fmt = Fmt { bold: true, size: 0, color: Some(HEAD_FG), fill: Some(HEAD_BG), h: None, num: None };
const IN: Fmt = Fmt { color: Some(INPUT), ..PLAIN };
const MUTE: Fmt = Fmt { color: Some(MUTED), ..PLAIN };

fn put(wb: &mut Workbook, a1: &str, input: &str, f: Fmt) {
    let p = parse_a1(a1).expect("cell");
    wb.set_input(0, p, input);
    let apply = move |s: &mut Style| {
        if f.bold { s.bold = true; }
        if f.size > 0 { s.size = f.size * 10; }
        if let Some(c) = f.color { s.color = Some(c); }
        if let Some(c) = f.fill { s.fill = Some(c); }
        if let Some(h) = f.h { s.h = h; }
        if let Some(n) = f.num { s.num_fmt = n.into(); }
    };
    wb.apply_style(0, &[Range::cell(p)], &apply);
}

fn col(i: u32) -> String { crate::addr::col_name(i) }

fn today() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    secs.div_euclid(86400)
}
fn iso(days: i64) -> String { let (y, m, d) = crate::date::civil_from_days(days); format!("{y:04}-{m:02}-{d:02}") }

fn sheet(name: &str, widths: &[(u32, f32)]) -> Workbook {
    let mut wb = Workbook::new();
    wb.sheets[0].name = name.into();
    for (c, w) in widths { wb.sheets[0].col_widths.insert(*c, *w); }
    wb
}

fn invoice() -> Workbook {
    let mut wb = sheet("Invoice", &[(0, 280.0), (1, 80.0), (2, 120.0), (3, 130.0)]);
    let money = Fmt { num: Some(MONEY), ..PLAIN };
    put(&mut wb, "A1", "INVOICE", Fmt { bold: true, size: 24, ..PLAIN });
    put(&mut wb, "A3", "From", Fmt { bold: true, ..MUTE });
    for (a, t) in [("A4", "Your company"), ("A5", "123 Main Street, Springfield"), ("A6", "hello@example.com"), ("C4", "Client name"), ("C5", "Client address"), ("C6", "client@example.com")] { put(&mut wb, a, t, IN); }
    put(&mut wb, "C3", "Bill to", Fmt { bold: true, ..MUTE });
    put(&mut wb, "A8", "Invoice number", MUTE);
    put(&mut wb, "B8", "1001", Fmt { h: Some(HAlign::Left), ..IN });
    put(&mut wb, "C8", "Invoice date", MUTE);
    put(&mut wb, "D8", "=TODAY()", Fmt { num: Some("mmm d, yyyy"), ..IN });
    put(&mut wb, "C9", "Due date", MUTE);
    put(&mut wb, "D9", "=D8+30", Fmt { num: Some("mmm d, yyyy"), ..PLAIN });
    for (a, t) in [("A11", "Description"), ("B11", "Qty"), ("C11", "Unit price"), ("D11", "Amount")] { put(&mut wb, a, t, Fmt { h: if a == "A11" { None } else { Some(HAlign::Right) }, ..HEAD }); }
    let items = [("Design work", 12, 95), ("Development", 30, 120), ("Hosting (monthly)", 1, 25)];
    for i in 0..8 {
        let r = 12 + i;
        if let Some((d, q, p)) = items.get(i) {
            put(&mut wb, &format!("A{r}"), d, IN);
            put(&mut wb, &format!("B{r}"), &q.to_string(), IN);
            put(&mut wb, &format!("C{r}"), &p.to_string(), Fmt { num: Some(MONEY), ..IN });
        }
        put(&mut wb, &format!("D{r}"), &format!("=IF(OR(B{r}=\"\",C{r}=\"\"),\"\",B{r}*C{r})"), money);
    }
    put(&mut wb, "C21", "Subtotal", Fmt { h: Some(HAlign::Right), ..PLAIN });
    put(&mut wb, "D21", "=SUM(D12:D19)", money);
    put(&mut wb, "B22", "Tax rate", Fmt { h: Some(HAlign::Right), ..PLAIN });
    put(&mut wb, "C22", "8%", Fmt { num: Some("0.0%"), ..IN });
    put(&mut wb, "D22", "=D21*C22", money);
    put(&mut wb, "C23", "Total due", Fmt { bold: true, h: Some(HAlign::Right), ..PLAIN });
    put(&mut wb, "D23", "=D21+D22", Fmt { bold: true, ..money });
    put(&mut wb, "A25", "Notes", Fmt { bold: true, ..MUTE });
    put(&mut wb, "A26", "Payment is due within 30 days. Thank you for your business.", IN);
    wb
}

fn todo() -> Workbook {
    let mut wb = sheet("Tasks", &[(0, 60.0), (1, 300.0), (2, 110.0), (3, 100.0), (4, 260.0), (5, 30.0), (6, 130.0), (7, 80.0)]);
    for (i, h) in ["Done", "Task", "Due", "Priority", "Notes"].iter().enumerate() { put(&mut wb, &format!("{}1", col(i as u32)), h, Fmt { h: Some(if i == 0 { HAlign::Center } else { HAlign::Left }), ..HEAD }); }
    let t = today();
    let rows = [(true, "Set up the project board", -3, "High", ""), (false, "Write the kickoff brief", 0, "High", "Share with the team by noon"), (false, "Review the budget draft", 1, "Medium", ""), (false, "Book the venue", 3, "Medium", "Two options shortlisted"), (false, "Send invitations", 7, "Low", "After the venue is confirmed"), (false, "Order name badges", 10, "Low", "")];
    for (i, (done, task, off, pri, notes)) in rows.iter().enumerate() {
        let r = 2 + i;
        put(&mut wb, &format!("A{r}"), if *done { "TRUE" } else { "FALSE" }, Fmt { h: Some(HAlign::Center), ..PLAIN });
        put(&mut wb, &format!("B{r}"), task, PLAIN);
        put(&mut wb, &format!("C{r}"), &iso(t + off), Fmt { num: Some("mmm d, yyyy"), ..PLAIN });
        put(&mut wb, &format!("D{r}"), pri, PLAIN);
        if !notes.is_empty() { put(&mut wb, &format!("E{r}"), notes, MUTE); }
    }
    put(&mut wb, "G1", "Summary", HEAD); put(&mut wb, "H1", "", HEAD);
    put(&mut wb, "G2", "Open", PLAIN); put(&mut wb, "H2", "=COUNTIFS(B2:B200,\"<>\",A2:A200,\"<>TRUE\")", PLAIN);
    put(&mut wb, "G3", "Done", PLAIN); put(&mut wb, "H3", "=COUNTIF(A2:A200,TRUE)", PLAIN);
    put(&mut wb, "G4", "Overdue", PLAIN); put(&mut wb, "H4", "=COUNTIFS(C2:C200,\"<\"&TODAY(),A2:A200,\"<>TRUE\")", PLAIN);
    put(&mut wb, "G5", "Due this week", PLAIN); put(&mut wb, "H5", "=COUNTIFS(C2:C200,\">=\"&TODAY(),C2:C200,\"<=\"&TODAY()+7,A2:A200,\"<>TRUE\")", PLAIN);
    wb.sheets[0].frozen_rows = 1;
    wb
}

fn loan() -> Workbook {
    let mut wb = sheet("Loan", &[(0, 160.0), (1, 130.0), (2, 120.0), (3, 120.0), (4, 120.0), (5, 130.0)]);
    let money = Fmt { num: Some(MONEY), ..PLAIN };
    let (y, m, _) = crate::date::civil_from_days(today());
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    put(&mut wb, "A1", "Loan calculator", Fmt { bold: true, size: 18, ..PLAIN });
    put(&mut wb, "A2", "Change the blue cells; everything else updates.", MUTE);
    put(&mut wb, "A4", "Loan amount", PLAIN); put(&mut wb, "B4", "250000", Fmt { num: Some("\"$\"#,##0"), ..IN });
    put(&mut wb, "A5", "Annual interest rate", PLAIN); put(&mut wb, "B5", "6.5%", Fmt { num: Some("0.00%"), ..IN });
    put(&mut wb, "A6", "Term (years)", PLAIN); put(&mut wb, "B6", "30", IN);
    put(&mut wb, "A7", "First payment", PLAIN); put(&mut wb, "B7", &format!("{ny:04}-{nm:02}-01"), Fmt { num: Some("mmm d, yyyy"), ..IN });
    put(&mut wb, "D4", "Monthly payment", Fmt { bold: true, ..PLAIN }); put(&mut wb, "E4", "=PMT(B5/12,B6*12,-B4)", Fmt { bold: true, ..money });
    put(&mut wb, "D5", "Number of payments", PLAIN); put(&mut wb, "E5", "=B6*12", PLAIN);
    put(&mut wb, "D6", "Total paid", PLAIN); put(&mut wb, "E6", "=E4*E5", money);
    put(&mut wb, "D7", "Total interest", PLAIN); put(&mut wb, "E7", "=E6-B4", money);
    put(&mut wb, "D8", "Last payment", PLAIN); put(&mut wb, "E8", "=EDATE(B7,E5-1)", Fmt { num: Some("mmm d, yyyy"), ..PLAIN });
    for (i, h) in ["#", "Date", "Payment", "Interest", "Principal", "Balance"].iter().enumerate() { put(&mut wb, &format!("{}10", col(i as u32)), h, Fmt { h: Some(if i < 2 { HAlign::Left } else { HAlign::Right }), ..HEAD }); }
    for i in 0..60 {
        let r = 11 + i;
        put(&mut wb, &format!("A{r}"), &format!("=IF({}<=$E$5,{},\"\")", i + 1, i + 1), PLAIN);
        put(&mut wb, &format!("B{r}"), &format!("=IF(A{r}=\"\",\"\",EDATE($B$7,A{r}-1))"), Fmt { num: Some("mmm d, yyyy"), ..PLAIN });
        put(&mut wb, &format!("C{r}"), &format!("=IF(A{r}=\"\",\"\",$E$4)"), money);
        put(&mut wb, &format!("D{r}"), &format!("=IF(A{r}=\"\",\"\",IPMT($B$5/12,A{r},$E$5,-$B$4))"), money);
        put(&mut wb, &format!("E{r}"), &format!("=IF(A{r}=\"\",\"\",C{r}-D{r})"), money);
        let bal = if i == 0 { format!("=IF(A{r}=\"\",\"\",MAX(0,$B$4-E{r}))") } else { format!("=IF(A{r}=\"\",\"\",MAX(0,F{}-E{r}))", r - 1) };
        put(&mut wb, &format!("F{r}"), &bal, money);
    }
    put(&mut wb, "A72", "The schedule shows the first 60 payments.", MUTE);
    wb.sheets[0].frozen_rows = 10;
    wb
}

fn gradebook() -> Workbook {
    let mut wb = sheet("Grades", &[(0, 170.0), (1, 90.0), (2, 90.0), (3, 90.0), (4, 90.0), (5, 100.0), (6, 80.0)]);
    for (i, h) in ["Student", "Homework", "Quiz", "Midterm", "Final", "Average", "Grade"].iter().enumerate() { put(&mut wb, &format!("{}1", col(i as u32)), h, Fmt { h: Some(if i > 0 { HAlign::Right } else { HAlign::Left }), ..HEAD }); }
    put(&mut wb, "A2", "Weight", Fmt { bold: true, ..MUTE });
    for (i, w) in ["20%", "10%", "30%", "40%"].iter().enumerate() { put(&mut wb, &format!("{}2", col(1 + i as u32)), w, Fmt { bold: true, num: Some("0%"), ..IN }); }
    let students = [("Ava Martin", [92, 88, 79, 85]), ("Ben Okafor", [78, 82, 71, 74]), ("Chloe Dubois", [96, 91, 94, 97]), ("Diego Alvarez", [65, 70, 58, 62]), ("Emma Schulz", [88, 79, 83, 90]), ("Farah Haddad", [71, 64, 69, 73]), ("Grace Kim", [84, 90, 88, 81]), ("Hugo Rossi", [59, 61, 52, 57]), ("Isla Murphy", [90, 95, 91, 89]), ("Jamal Wright", [82, 77, 80, 84])];
    for (i, (name, scores)) in students.iter().enumerate() {
        let r = 3 + i;
        put(&mut wb, &format!("A{r}"), name, PLAIN);
        for (j, s) in scores.iter().enumerate() { put(&mut wb, &format!("{}{r}", col(1 + j as u32)), &s.to_string(), IN); }
        put(&mut wb, &format!("F{r}"), &format!("=IF(COUNT(B{r}:E{r})=0,\"\",SUMPRODUCT(B{r}:E{r},$B$2:$E$2)/SUMPRODUCT(--(B{r}:E{r}<>\"\"),$B$2:$E$2))"), Fmt { num: Some("0.0"), ..PLAIN });
        put(&mut wb, &format!("G{r}"), &format!("=IF(F{r}=\"\",\"\",IFS(F{r}>=90,\"A\",F{r}>=80,\"B\",F{r}>=70,\"C\",F{r}>=60,\"D\",TRUE,\"F\"))"), Fmt { bold: true, h: Some(HAlign::Center), ..PLAIN });
    }
    let last = 2 + students.len();
    for (k, (label, func)) in [("Class average", "AVERAGE"), ("Highest", "MAX"), ("Lowest", "MIN")].iter().enumerate() {
        let r = last + 2 + k;
        put(&mut wb, &format!("A{r}"), label, Fmt { bold: k == 0, ..PLAIN });
        for c in ["B", "C", "D", "E", "F"] { put(&mut wb, &format!("{c}{r}"), &format!("={func}({c}3:{c}{last})"), Fmt { bold: k == 0, num: Some("0.0"), ..PLAIN }); }
    }
    wb.sheets[0].frozen_rows = 2; wb.sheets[0].frozen_cols = 1;
    wb
}

fn schedule() -> Workbook {
    let mut wb = sheet("Week", &[(0, 80.0), (1, 150.0), (2, 150.0), (3, 150.0), (4, 150.0), (5, 150.0), (6, 150.0), (7, 150.0)]);
    let t = today();
    // 1970-01-01 was a Thursday.
    let monday = t - (t + 3).rem_euclid(7);
    put(&mut wb, "A1", "Week of", Fmt { h: Some(HAlign::Right), ..HEAD });
    put(&mut wb, "B1", &iso(monday), Fmt { bold: true, num: Some("mmm d, yyyy"), ..IN });
    put(&mut wb, "A2", "Time", HEAD);
    for (i, d) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() { put(&mut wb, &format!("{}2", col(1 + i as u32)), &format!("=\"{d} \"&TEXT($B$1+{i},\"d\")"), Fmt { h: Some(HAlign::Center), ..HEAD }); }
    for h in 7..=20 { put(&mut wb, &format!("A{}", h - 4), &format!("{h:02}:00"), Fmt { h: Some(HAlign::Right), num: Some("hh:mm"), ..MUTE }); }
    for (a, t) in [("B5", "Team stand-up"), ("B7", "Deep work"), ("C6", "Design review"), ("D5", "Team stand-up"), ("D9", "Dentist"), ("E8", "Lunch with Sam"), ("F5", "Team stand-up"), ("F10", "Weekly retro"), ("G6", "Farmers market"), ("H8", "Family call")] {
        put(&mut wb, a, t, Fmt { fill: Some([0xe0, 0xec, 0xff]), color: Some(HEAD_FG), ..PLAIN });
    }
    for r in 2..=15 { wb.sheets[0].row_heights.insert(r, 34.0); }
    wb.sheets[0].frozen_rows = 2; wb.sheets[0].frozen_cols = 1;
    wb
}

fn habits() -> Workbook {
    let days = 31u32;
    let mut widths = vec![(0, 170.0)];
    for i in 0..days { widths.push((1 + i, 30.0)); }
    widths.push((1 + days, 70.0)); widths.push((2 + days, 80.0));
    let mut wb = sheet("Habits", &widths);
    let (y, m, _) = crate::date::civil_from_days(today());
    put(&mut wb, "A1", "Habit tracker", Fmt { bold: true, size: 18, ..PLAIN });
    put(&mut wb, "A2", &format!("{y:04}-{m:02}-01"), Fmt { bold: true, num: Some("mmmm yyyy"), ..IN });
    put(&mut wb, "A3", "Habit", HEAD);
    for i in 0..days {
        let c = col(1 + i);
        put(&mut wb, &format!("{c}2"), &format!("=IF(DAY($A$2+{i})={},LEFT(TEXT($A$2+{i},\"ddd\"),1),\"\")", i + 1), Fmt { h: Some(HAlign::Center), size: 9, ..MUTE });
        put(&mut wb, &format!("{c}3"), &format!("=IF(DAY($A$2+{i})={},{},\"\")", i + 1, i + 1), Fmt { h: Some(HAlign::Center), size: 10, ..HEAD });
    }
    put(&mut wb, &format!("{}3", col(1 + days)), "Done", Fmt { h: Some(HAlign::Right), ..HEAD });
    put(&mut wb, &format!("{}3", col(2 + days)), "Rate", Fmt { h: Some(HAlign::Right), ..HEAD });
    for (i, h) in ["Exercise", "Read 20 minutes", "No sugar", "Sleep by 11", "Write journal", "Walk 10k steps"].iter().enumerate() {
        let r = 4 + i;
        put(&mut wb, &format!("A{r}"), h, IN);
        for d in 0..10u32 { if (d as usize + i) % 3 == 0 { put(&mut wb, &format!("{}{r}", col(1 + d)), "TRUE", Fmt { h: Some(HAlign::Center), ..PLAIN }); } }
        put(&mut wb, &format!("{}{r}", col(1 + days)), &format!("=COUNTIF(B{r}:{}{r},TRUE)", col(days)), Fmt { h: Some(HAlign::Right), ..PLAIN });
        put(&mut wb, &format!("{}{r}", col(2 + days)), &format!("=IFERROR({}{r}/MAX(1,COUNTIF(B$3:{}$3,\">0\")),0)", col(1 + days), col(days)), Fmt { h: Some(HAlign::Right), num: Some("0%"), ..PLAIN });
    }
    wb.sheets[0].frozen_rows = 3; wb.sheets[0].frozen_cols = 1;
    wb
}

/// A fresh workbook from a template, recalculated and with its first cell selected.
pub fn build(id: &str) -> Option<Workbook> {
    let mut wb = match id { "blank" => Workbook::new(), "invoice" => invoice(), "todo" => todo(), "loan" => loan(), "gradebook" => gradebook(), "schedule" => schedule(), "habits" => habits(), _ => return None };
    wb.sheets[0].selection = Some(Range::cell(Pos::new(0, 0)));
    wb.recalc();
    Some(wb)
}

#[cfg(test)]
mod tests {
    #[test]
    fn templates_calculate() {
        for (id, ..) in super::TEMPLATES {
            let wb = super::build(id).unwrap();
            for s in &wb.sheets { for c in s.cells.values() { assert!(!matches!(c.value, crate::Value::Err(crate::ErrKind::Name | crate::ErrKind::Ref)), "{id}: {:?}", c.formula()); } }
        }
        let loan = super::build("loan").unwrap();
        let pay = loan.sheets[0].value(crate::Pos::new(3, 4));
        assert!(matches!(pay, crate::Value::Num(x) if (x - 1580.17).abs() < 0.01), "{pay:?}");
    }
}
