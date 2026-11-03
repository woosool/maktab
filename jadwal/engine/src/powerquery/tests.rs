use super::eval::{Ctx, Table, V};
use super::*;
use crate::addr::{parse_range, Pos};

fn t(s: &str) -> V { V::text(s) }
fn n(x: f64) -> V { V::Num(x) }
fn no_web(_: &str) -> Result<String, String> { Err("offline".into()) }

/// A workbook with a table Sales (Region, Product, Units, Price) on Sheet1.
fn sales_book() -> Workbook {
    let mut wb = Workbook::new();
    let rows = [["Region", "Product", "Units", "Price"], ["East", "Pen", "10", "1.5"], ["West", "Pen", "4", "1.5"], ["East", "Ink", "", "7"], ["North", "Pad", "3", "2.25"], ["West", "Ink", "6", "7"]];
    for (r, row) in rows.iter().enumerate() { for (c, v) in row.iter().enumerate() { if !v.is_empty() { wb.set_input(0, Pos::new(r as u32, c as u32), v); } } }
    let ti = wb.create_table(0, parse_range("A1:D6").unwrap(), true, "TableStyleMedium2").unwrap();
    wb.rename_table(0, ti, "Sales").unwrap();
    wb
}

fn run(wb: &Workbook, q: &Query) -> Table { Ctx::new(wb, &[], &no_web).run(q, None).unwrap() }
fn with(op: Op) -> Query { let mut q = Query::new("Q", Source::Current { name: "Sales".into() }); q.add_step(op, None); q }
fn col(t: &Table, name: &str) -> Vec<V> { let i = t.col(name).unwrap(); t.rows.iter().map(|r| r[i].clone()).collect() }

#[test]
fn source_and_column_steps() {
    let wb = sales_book();
    let base = run(&wb, &Query::new("Q", Source::Current { name: "Sales".into() }));
    assert_eq!(base.columns, ["Region", "Product", "Units", "Price"]);
    assert_eq!(base.rows.len(), 5);
    assert_eq!(base.rows[2][2], V::Null);
    let r = run(&wb, &with(Op::RemoveColumns(vec!["Price".into()])));
    assert_eq!(r.columns, ["Region", "Product", "Units"]);
    let r = run(&wb, &with(Op::SelectColumns(vec!["Units".into(), "Region".into()])));
    assert_eq!(r.columns, ["Units", "Region"]);
    let r = run(&wb, &with(Op::RenameColumns(vec![("Units".into(), "Qty".into())])));
    assert_eq!(r.columns[2], "Qty");
    let r = run(&wb, &with(Op::ReorderColumns(vec!["Price".into(), "Region".into()])));
    assert_eq!(r.columns, ["Price", "Product", "Units", "Region"]);
    let r = run(&wb, &with(Op::ChangeType(vec![("Units".into(), Ty::Text), ("Price".into(), Ty::Int)])));
    assert_eq!(r.rows[0][2], t("10"));
    assert_eq!(r.rows[0][3], n(2.0)); // 1.5 rounds to even
    assert_eq!(r.rows[3][3], n(2.0)); // 2.25
    let r = run(&wb, &with(Op::ChangeType(vec![("Region".into(), Ty::Number)])));
    assert!(matches!(r.rows[0][0], V::Err(_)));
    // Errors in other steps say which column.
    assert!(Ctx::new(&wb, &[], &no_web).run(&with(Op::RemoveColumns(vec!["Nope".into()])), None).unwrap_err().contains("Nope"));
}

#[test]
fn row_steps() {
    let wb = sales_book();
    let f = run(&wb, &with(Op::FilterRows(conds_expr("Region", (CondOp::Equals, t("East")), Some((CondOp::Equals, t("North"))), false))));
    assert_eq!(col(&f, "Region"), [t("East"), t("East"), t("North")]);
    let f = run(&wb, &with(Op::FilterRows(cond_expr("Units", CondOp::Greater, &n(4.0)))));
    assert_eq!(col(&f, "Units"), [n(10.0), n(6.0)]);
    let f = run(&wb, &with(Op::FilterRows(cond_expr("Product", CondOp::BeginsWith, &t("P")))));
    assert_eq!(f.rows.len(), 3);
    let f = run(&wb, &with(Op::FilterRows(values_expr("Region", &[t("East"), t("West")], &[t("North")]))));
    assert_eq!(f.rows.len(), 4);
    let s = run(&wb, &with(Op::Sort(vec![("Region".into(), false), ("Units".into(), true)])));
    assert_eq!(col(&s, "Region")[..3], [t("East"), t("East"), t("North")]);
    assert_eq!(col(&s, "Units")[..2], [n(10.0), V::Null]);
    assert_eq!(run(&wb, &with(Op::FirstN(2))).rows.len(), 2);
    assert_eq!(col(&run(&wb, &with(Op::LastN(1))), "Region"), [t("West")]);
    assert_eq!(col(&run(&wb, &with(Op::Skip(4))), "Region"), [t("West")]);
    assert_eq!(run(&wb, &with(Op::RemoveLastN(2))).rows.len(), 3);
    assert_eq!(run(&wb, &with(Op::Distinct(Some(vec!["Region".into()])))).rows.len(), 3);
    assert_eq!(run(&wb, &with(Op::Distinct(None))).rows.len(), 5);
    assert_eq!(col(&run(&wb, &with(Op::FillDown(vec!["Units".into()]))), "Units")[2], n(4.0));
    assert_eq!(col(&run(&wb, &with(Op::FillUp(vec!["Units".into()]))), "Units")[2], n(3.0));
    let r = run(&wb, &with(Op::ReplaceValue { find: t("Pen"), replace: t("Pencil"), text: false, columns: vec!["Product".into()] }));
    assert_eq!(col(&r, "Product")[0], t("Pencil"));
    let r = run(&wb, &with(Op::ReplaceValue { find: t("e"), replace: t("3"), text: true, columns: vec!["Product".into()] }));
    assert_eq!(col(&r, "Product")[0], t("P3n"));
    let r = run(&wb, &with(Op::ReplaceValue { find: V::Null, replace: n(0.0), text: false, columns: vec!["Units".into()] }));
    assert_eq!(col(&r, "Units")[2], n(0.0));
}

#[test]
fn shaping_steps() {
    let wb = sales_book();
    let mut q = with(Op::MergeColumns { columns: vec!["Region".into(), "Product".into()], separator: "-".into(), name: "Key".into() });
    let r = run(&wb, &q);
    assert_eq!(r.columns, ["Key", "Units", "Price"]);
    assert_eq!(r.rows[0][0], t("East-Pen"));
    q.add_step(Op::SplitColumn { column: "Key".into(), by: Split::Delimiter { delimiter: "-".into(), mode: SplitMode::Each }, into: vec!["Key.1".into(), "Key.2".into()] }, None);
    let r = run(&wb, &q);
    assert_eq!(r.columns, ["Key.1", "Key.2", "Units", "Price"]);
    assert_eq!(r.rows[1][1], t("Pen"));
    let r = run(&wb, &with(Op::SplitColumn { column: "Region".into(), by: Split::Positions(vec![0, 2]), into: vec!["A".into(), "B".into()] }));
    assert_eq!((r.rows[0][0].clone(), r.rows[0][1].clone()), (t("Ea"), t("st")));
    let r = run(&wb, &with(Op::AddIndex { name: "Index".into(), start: 1, step: 1 }));
    assert_eq!(col(&r, "Index"), [n(1.0), n(2.0), n(3.0), n(4.0), n(5.0)]);
    let e = mlang::parse("if [Units] = null then \"none\" else Text.Upper([Product]) & \"/\" & Text.From(Number.Round([Units] * [Price], 1))").unwrap();
    let r = run(&wb, &with(Op::AddColumn { name: "Custom".into(), expr: e, ty: None }));
    assert_eq!(col(&r, "Custom")[..3], [t("PEN/15"), t("PEN/6"), t("none")]);
    let e = mlang::parse("Text.Length([Region]) + Date.Year(#date(2024, 3, 9)) - Date.Month(#date(2024, 3, 9)) * Date.Day(#date(2024, 3, 9)) > 2000 and Text.Start([Region], 1) <> Text.End(\"xE\", 1)").unwrap();
    let r = run(&wb, &with(Op::AddColumn { name: "B".into(), expr: e, ty: Some(Ty::Logical) }));
    assert_eq!(col(&r, "B")[..3], [V::Bool(false), V::Bool(true), V::Bool(false)]);
    let r = run(&wb, &with(Op::AddColumn { name: "T".into(), expr: mlang::parse("Text.Trim(\"  a \") & Text.Lower(\"B\")").unwrap(), ty: None }));
    assert_eq!(col(&r, "T")[0], t("ab"));
    let g = run(&wb, &with(Op::Group { keys: vec!["Region".into()], aggs: vec![
        Agg { name: "Count".into(), op: AggOp::CountRows, column: String::new() }, Agg { name: "Units".into(), op: AggOp::Sum, column: "Units".into() },
        Agg { name: "Avg".into(), op: AggOp::Average, column: "Price".into() }, Agg { name: "Min".into(), op: AggOp::Min, column: "Price".into() }, Agg { name: "Max".into(), op: AggOp::Max, column: "Units".into() }] }));
    assert_eq!(g.columns, ["Region", "Count", "Units", "Avg", "Min", "Max"]);
    assert_eq!(g.rows[0], vec![t("East"), n(2.0), n(10.0), n(4.25), n(1.5), n(10.0)]);
    assert_eq!(g.rows[1][2], n(10.0));
    let p = run(&wb, &with(Op::Pivot { column: "Product".into(), values: "Units".into(), agg: Some(AggOp::Sum) }));
    assert_eq!(p.columns, ["Region", "Price", "Pen", "Ink", "Pad"]);
    let mut q = with(Op::RemoveColumns(vec!["Price".into()]));
    q.add_step(Op::Pivot { column: "Product".into(), values: "Units".into(), agg: Some(AggOp::Sum) }, None);
    let p = run(&wb, &q);
    assert_eq!(p.rows[0], vec![t("East"), n(10.0), V::Null, V::Null]);
    q.add_step(Op::Unpivot { columns: vec!["Region".into()], other: true, attribute: "Attribute".into(), value: "Value".into() }, None);
    let u = run(&wb, &q);
    assert_eq!(u.columns, ["Region", "Attribute", "Value"]);
    assert_eq!(u.rows[0], vec![t("East"), t("Pen"), n(10.0)]);
    let u = run(&wb, &with(Op::Unpivot { columns: vec!["Units".into(), "Price".into()], other: false, attribute: "A".into(), value: "V".into() }));
    assert_eq!(u.rows.len(), 9);
    let tr = run(&wb, &with(Op::Transpose));
    assert_eq!((tr.columns.len(), tr.rows.len()), (5, 4));
    assert_eq!(tr.rows[0][0], t("East"));
    let mut q = with(Op::DemoteHeaders);
    q.add_step(Op::PromoteHeaders, None);
    assert_eq!(run(&wb, &q).columns, ["Region", "Product", "Units", "Price"]);
}

#[test]
fn merge_and_append_queries() {
    let mut wb = sales_book();
    for (r, row) in [["Product", "Color"], ["Pen", "Blue"], ["Ink", "Black"], ["Cap", "Red"]].iter().enumerate() { for (c, v) in row.iter().enumerate() { wb.set_input(0, Pos::new(r as u32, 6 + c as u32), v); } }
    let ti = wb.create_table(0, parse_range("G1:H4").unwrap(), true, "").unwrap();
    wb.rename_table(0, ti, "Colors").unwrap();
    let colors = Query::new("Colors", Source::Current { name: "Colors".into() });
    let all = vec![colors];
    let join = |kind: Join| {
        let mut q = with(Op::NestedJoin { keys: vec!["Product".into()], other: "Colors".into(), other_keys: vec!["Product".into()], column: "Colors".into(), kind });
        q.add_step(Op::ExpandTable { column: "Colors".into(), fields: vec!["Color".into()], names: vec!["Colors.Color".into()] }, None);
        Ctx::new(&wb, &all, &no_web).run(&q, None).unwrap()
    };
    let l = join(Join::LeftOuter);
    assert_eq!(col(&l, "Colors.Color"), [t("Blue"), t("Blue"), t("Black"), V::Null, t("Black")]);
    assert_eq!(join(Join::Inner).rows.len(), 4);
    assert_eq!(join(Join::LeftAnti).rows.len(), 1);
    let r = join(Join::RightOuter);
    assert_eq!(r.rows.len(), 5);
    assert_eq!(r.rows[4][0], V::Null);
    assert_eq!(join(Join::FullOuter).rows.len(), 6);
    assert_eq!(join(Join::RightAnti).rows.len(), 1);
    let a = Ctx::new(&wb, &all, &no_web).run(&with(Op::Append(vec!["Colors".into()])), None).unwrap();
    assert_eq!(a.columns, ["Region", "Product", "Units", "Price", "Color"]);
    assert_eq!(a.rows.len(), 8);
    assert_eq!(a.rows[7][1], t("Cap"));
    // A query referring to itself is reported, not looped.
    let me = vec![Query::new("Me", Source::Query("Me".into()))];
    assert!(Ctx::new(&wb, &me, &no_web).run(&me[0], None).unwrap_err().contains("cyclic"));
}

#[test]
fn csv_web_and_workbook_sources() {
    let dir = std::env::temp_dir().join(format!("jadwal-pq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("d.csv");
    std::fs::write(&csv, "name,qty\n\"a, b\",2\nc,3\n").unwrap();
    let wb = Workbook::new();
    let mut q = Query::new("D", Source::Csv { path: csv.to_string_lossy().into(), delimiter: ",".into(), encoding: 65001, columns: Some(2) });
    q.add_step(Op::PromoteHeaders, None);
    detect_types(&wb, &mut q);
    let r = run(&wb, &q);
    assert_eq!(r.columns, ["name", "qty"]);
    assert_eq!(r.types, [Ty::Text, Ty::Int]);
    assert_eq!(r.rows[0], vec![t("a, b"), n(2.0)]);
    let web = |_: &str| -> Result<String, String> { Ok("<table><tr><th>k</th><th>v</th></tr><tr><td>x</td><td>9</td></tr></table>".into()) };
    let w = Ctx::new(&wb, &[], &web).run(&Query::new("W", Source::Web { url: "http://example.com".into(), index: 0 }), None).unwrap();
    assert_eq!(w.columns, ["k", "v"]);
    let book = dir.join("b.xlsx");
    crate::save(&sales_book(), &book).unwrap();
    let x = run(&wb, &Query::new("X", Source::Workbook { path: book.to_string_lossy().into(), item: "Sales".into(), kind: "Table".into() }));
    assert_eq!((x.columns.len(), x.rows.len()), (4, 5));
    let s = run(&wb, &Query::new("S", Source::Workbook { path: book.to_string_lossy().into(), item: "Sheet1".into(), kind: "Sheet".into() }));
    assert_eq!(s.columns[0], "Column1");
    std::fs::remove_dir_all(&dir).ok();
}

/// Every step's M reads back as the same step.
#[test]
fn m_round_trips() {
    let ops = vec![
        Op::PromoteHeaders, Op::DemoteHeaders,
        Op::ChangeType(Ty::ALL.iter().enumerate().map(|(i, t)| (format!("c{i}"), *t)).collect()),
        Op::RemoveColumns(vec!["a".into(), "b \"q\"".into()]), Op::SelectColumns(vec!["a".into()]), Op::ReorderColumns(vec!["b".into(), "a".into()]),
        Op::RenameColumns(vec![("a".into(), "x".into()), ("b".into(), "y".into())]),
        Op::FilterRows(conds_expr("Unit Price", (CondOp::GreaterEq, n(2.5)), Some((CondOp::NotContains, t("x"))), true)),
        Op::FilterRows(values_expr("R", &[t("East"), t("West")], &[])),
        Op::Sort(vec![("a".into(), false), ("b".into(), true)]),
        Op::ReplaceValue { find: t("a"), replace: V::Null, text: false, columns: vec!["a".into()] },
        Op::ReplaceValue { find: t("a"), replace: t("b"), text: true, columns: vec!["a".into(), "b".into()] },
        Op::SplitColumn { column: "a".into(), by: Split::Delimiter { delimiter: ",".into(), mode: SplitMode::Each }, into: vec!["a.1".into(), "a.2".into()] },
        Op::SplitColumn { column: "a".into(), by: Split::Delimiter { delimiter: " ".into(), mode: SplitMode::Right }, into: vec!["a.1".into(), "a.2".into()] },
        Op::SplitColumn { column: "a".into(), by: Split::Positions(vec![0, 3, 5]), into: vec!["a.1".into(), "a.2".into(), "a.3".into()] },
        Op::MergeColumns { columns: vec!["a".into(), "b".into()], separator: " ".into(), name: "Merged".into() },
        Op::AddColumn { name: "Custom".into(), expr: mlang::parse("if [a] > 1 then Text.Upper([b]) & \"!\" else null").unwrap(), ty: None },
        Op::AddColumn { name: "N".into(), expr: mlang::parse("[a] * 2").unwrap(), ty: Some(Ty::Number) },
        Op::AddIndex { name: "Index".into(), start: 1, step: 1 },
        Op::Group { keys: vec!["a".into()], aggs: vec![Agg { name: "Count".into(), op: AggOp::CountRows, column: String::new() }, Agg { name: "S".into(), op: AggOp::Sum, column: "b".into() }, Agg { name: "D".into(), op: AggOp::CountDistinct, column: "b".into() }, Agg { name: "M".into(), op: AggOp::Max, column: "Unit Price".into() }] },
        Op::Pivot { column: "a".into(), values: "b".into(), agg: Some(AggOp::Sum) }, Op::Pivot { column: "a".into(), values: "b".into(), agg: None },
        Op::Unpivot { columns: vec!["a".into()], other: true, attribute: "Attribute".into(), value: "Value".into() },
        Op::Unpivot { columns: vec!["a".into(), "b".into()], other: false, attribute: "Attribute".into(), value: "Value".into() },
        Op::NestedJoin { keys: vec!["k".into()], other: "Other Query".into(), other_keys: vec!["k".into()], column: "Other Query".into(), kind: Join::LeftAnti },
        Op::ExpandTable { column: "O".into(), fields: vec!["x".into()], names: vec!["O.x".into()] },
        Op::Append(vec!["Q2".into(), "Q 3".into()]),
        Op::Distinct(None), Op::Distinct(Some(vec!["a".into()])),
        Op::FirstN(5), Op::LastN(2), Op::Skip(1), Op::RemoveLastN(3), Op::FillDown(vec!["a".into()]), Op::FillUp(vec!["a".into()]), Op::Transpose,
    ];
    let mut q = Query::new("Sales Data", Source::Current { name: "Table1".into() });
    for op in &ops { q.add_step(op.clone(), None); }
    let m = q.formula();
    assert!(m.starts_with("let\r\n    Source = Excel.CurrentWorkbook(){[Name=\"Table1\"]}[Content],\r\n    #\"Promoted Headers\" = Table.PromoteHeaders(Source, [PromoteAllScalars=true]),"), "{m}");
    assert!(m.contains("#\"Changed Type\" = Table.TransformColumnTypes(#\"Demoted Headers\",{{\"c0\", type number}, {\"c1\", Currency.Type}, {\"c2\", Int64.Type}"), "{m}");
    assert!(m.contains("#\"Filtered Rows\" = Table.SelectRows(#\"Renamed Columns\", each [Unit Price] >= 2.5 and not Text.Contains([Unit Price], \"x\"))"), "{m}");
    assert!(m.contains("#\"Grouped Rows\" = Table.Group(#\"Added Index\", {\"a\"}, {{\"Count\", each Table.RowCount(_), Int64.Type}, {\"S\", each List.Sum([b]), type nullable number}"), "{m}");
    assert!(m.contains("Table.Pivot(#\"Grouped Rows\", List.Distinct(#\"Grouped Rows\"[a]), \"a\", \"b\", List.Sum)"), "{m}");
    assert!(m.contains("Table.NestedJoin(#\"Unpivoted Columns\", {\"k\"}, #\"Other Query\", {\"k\"}, \"Other Query\", JoinKind.LeftAnti)"), "{m}");
    assert!(m.contains("#\"Sorted Rows\" = Table.Sort(#\"Filtered Rows1\",{{\"a\", Order.Ascending}, {\"b\", Order.Descending}})"), "{m}");
    assert!(m.ends_with("in\r\n    #\"Transposed Table\""));
    let back = Query::from_formula("Sales Data", &m);
    assert!(!back.read_only(), "{:?}", mlang::parse(&m).and_then(|e| steps_from(&e)).err());
    assert_eq!(back.steps, q.steps);
    assert_eq!(back.formula(), m);
    for src in [Source::Csv { path: "C:\\data\\x.csv".into(), delimiter: ";".into(), encoding: 1252, columns: Some(3) }, Source::Workbook { path: "/a/b.xlsx".into(), item: "Sheet1".into(), kind: "Sheet".into() }, Source::Web { url: "https://e.com/p".into(), index: 2 }, Source::Query("Base Query".into())] {
        let q = Query::new("S", src.clone());
        assert_eq!(Query::from_formula("S", &q.formula()).source, Some(src));
    }
}

/// M as Excel's editor writes it (a CSV import, then edits) reads into steps.
#[test]
fn reads_excel_written_m() {
    let m = "let\r\n    Source = Csv.Document(File.Contents(\"C:\\Users\\me\\Sales.csv\"),[Delimiter=\",\", Columns=4, Encoding=65001, QuoteStyle=QuoteStyle.None]),\r\n    #\"Promoted Headers\" = Table.PromoteHeaders(Source, [PromoteAllScalars=true]),\r\n    #\"Changed Type\" = Table.TransformColumnTypes(#\"Promoted Headers\",{{\"Date\", type date}, {\"Region\", type text}, {\"Units\", Int64.Type}, {\"Price\", type number}}),\r\n    #\"Filtered Rows\" = Table.SelectRows(#\"Changed Type\", each ([Region] = \"East\" or [Region] = \"West\")),\r\n    #\"Sorted Rows\" = Table.Sort(#\"Filtered Rows\",{{\"Units\", Order.Descending}}),\r\n    #\"Added Custom\" = Table.AddColumn(#\"Sorted Rows\", \"Total\", each [Units] * [Price]),\r\n    #\"Grouped Rows\" = Table.Group(#\"Added Custom\", {\"Region\"}, {{\"Count\", each Table.RowCount(_), Int64.Type}, {\"Sum\", each List.Sum([Total]), type nullable number}}),\r\n    #\"Merged Queries\" = Table.NestedJoin(#\"Grouped Rows\", {\"Region\"}, Targets, {\"Region\"}, \"Targets\", JoinKind.LeftOuter),\r\n    #\"Expanded Targets\" = Table.ExpandTableColumn(#\"Merged Queries\", \"Targets\", {\"Target\"}, {\"Targets.Target\"}),\r\n    #\"Kept First Rows\" = Table.FirstN(#\"Expanded Targets\",10)\r\nin\r\n    #\"Kept First Rows\"";
    let q = Query::from_formula("Sales", m);
    assert!(!q.read_only(), "{:?}", mlang::parse(m).and_then(|e| steps_from(&e)).err());
    assert_eq!(q.steps.len(), 9);
    assert_eq!(q.source, Some(Source::Csv { path: "C:\\Users\\me\\Sales.csv".into(), delimiter: ",".into(), encoding: 65001, columns: Some(4) }));
    assert_eq!(q.steps[1].op, Op::ChangeType(vec![("Date".into(), Ty::Date), ("Region".into(), Ty::Text), ("Units".into(), Ty::Int), ("Price".into(), Ty::Number)]));
    assert_eq!(q.formula(), m);
    // A navigation step from Excel.Workbook, and a web table.
    let wbm = "let\r\n    Source = Excel.Workbook(File.Contents(\"C:\\b.xlsx\"), null, true),\r\n    Sheet1_Sheet = Source{[Item=\"Sheet1\",Kind=\"Sheet\"]}[Data],\r\n    #\"Promoted Headers\" = Table.PromoteHeaders(Sheet1_Sheet, [PromoteAllScalars=true])\r\nin\r\n    #\"Promoted Headers\"";
    let q = Query::from_formula("B", wbm);
    assert_eq!(q.source, Some(Source::Workbook { path: "C:\\b.xlsx".into(), item: "Sheet1".into(), kind: "Sheet".into() }));
    assert_eq!(q.formula(), wbm);
    // M Jadwal doesn't model stays as written, read-only.
    let other = "let\r\n    Source = Sql.Database(\"srv\", \"db\"),\r\n    dbo_T = Source{[Schema=\"dbo\",Item=\"T\"]}[Data]\r\nin\r\n    dbo_T";
    let q = Query::from_formula("Sql", other);
    assert!(q.read_only());
    assert_eq!(q.formula(), other);
}

#[test]
fn step_editing() {
    let mut q = Query::new("Q", Source::Current { name: "Sales".into() });
    q.add_step(Op::RemoveColumns(vec!["a".into()]), None);
    q.add_step(Op::RemoveColumns(vec!["b".into()]), None);
    assert_eq!(q.steps[1].name, "Removed Columns1");
    assert_eq!(q.step_formula(Some(1)), "Table.RemoveColumns(#\"Removed Columns\",{\"b\"})");
    q.set_step_formula(1, "= Table.SelectColumns(#\"Removed Columns\",{\"c\"})").unwrap();
    assert_eq!(q.steps[1].op, Op::SelectColumns(vec!["c".into()]));
    assert!(q.set_step_formula(1, "Table.SelectColumns(Source,{\"c\"})").is_err());
    assert!(q.rename_step(1, "Removed Columns").is_err());
    q.rename_step(0, "Dropped A").unwrap();
    assert!(q.formula().contains("Table.SelectColumns(#\"Dropped A\""));
    let i = q.add_step(Op::FirstN(1), Some(0));
    assert_eq!(i, 1);
    assert_eq!(q.steps[2].op, Op::SelectColumns(vec!["c".into()]));
}

#[test]
fn datamashup_stream_round_trips() {
    let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    for k in 0..4 { assert_eq!(mashup::b64_decode(&mashup::b64_encode(&data[k..])).unwrap(), data[k..].to_vec()); }
    let mut q = Query::new("My Sales", Source::Current { name: "Sales".into() });
    q.add_step(Op::FirstN(3), None);
    q.load = Load::Table("My_Sales".into());
    q.description = "Top rows".into();
    let s = mashup::stream_for(None, &[], &[q.clone(), Query::from_formula("Raw", "let\r\n    Source = Odbc.Query(\"dsn=x\", \"select 1\")\r\nin\r\n    Source")]);
    let bytes = mashup::encode(&s).unwrap();
    assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
    let back = mashup::decode(&bytes).unwrap();
    assert_eq!(back, s);
    assert_eq!(back.bindings, vec![0]);
    let names: Vec<&str> = back.parts.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(names, ["[Content_Types].xml", "Config/Package.xml", "Formulas/Section1.m"]);
    let m = mashup::from_stream(&back).unwrap();
    assert_eq!(m.queries.len(), 2);
    assert_eq!(m.queries[0].steps, q.steps);
    assert_eq!(m.queries[0].load, Load::Table("My_Sales".into()));
    assert_eq!(m.queries[0].description, "Top rows");
    assert!(m.queries[1].read_only());
    assert!(back.metadata_xml.contains("<ItemPath>Section1/My%20Sales</ItemPath>") && back.metadata_xml.contains("<Entry Type=\"FillTarget\" Value=\"sMy_Sales\" />") && back.metadata_xml.contains("<ItemPath>Section1/My%20Sales/Kept%20First%20Rows</ItemPath>"), "{}", back.metadata_xml);
}

/// The example of [MS-QDEFF] section 3 (a DataMashup Excel wrote for a query "=1+1" loaded to
/// a table and refreshed).
#[test]
fn reads_the_specs_excel_datamashup() {
    let b64 = include_str!("qdeff_example.b64");
    let s = mashup::decode(&mashup::b64_decode(b64).unwrap()).unwrap();
    assert_eq!(s.parts.len(), 3);
    assert_eq!(s.metadata_content.len(), 22);
    assert!(s.bindings.len() > 100);
    let m = mashup::from_stream(&s).unwrap();
    assert_eq!(m.queries.len(), 1);
    let q = &m.queries[0];
    assert_eq!(q.name, "Query1");
    assert!(q.read_only());
    assert_eq!(q.formula(), "let\r\n    Source = 1+1\r\nin\r\n    Source");
    assert_eq!(q.load, Load::Table("Query1".into()));
    assert!(q.meta.iter().any(|(k, v)| k == "QueryID" && v == "s7e04362e-92f5-4d82-8b07-2761eaf68ae5"));
    // Stored in a workbook (as Excel's UTF-16 custom XML part) it lists and survives a save.
    let mut wb = Workbook::new();
    let xml = format!("<?xml version=\"1.0\" encoding=\"utf-16\"?><DataMashup sqmid=\"7690c5d6-5698-463c-a560-a0093d4f6332\" xmlns=\"http://schemas.microsoft.com/DataMashup\">{}</DataMashup>", b64.trim());
    let mut b = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() { b.extend_from_slice(&u.to_le_bytes()); }
    wb.parts.insert("customXml/item1.xml".into(), b);
    assert_eq!(list(&wb).len(), 1);
    // Adding a query keeps the read one as written, and the root's attributes.
    let mut q2 = Query::new("Two", Source::Query("Query1".into()));
    q2.add_step(Op::FirstN(1), None);
    save(&mut wb, &q2, None).unwrap();
    let back = crate::xlsx::read(&crate::xlsx::write(&wb).unwrap()).unwrap();
    let qs = list(&back);
    assert_eq!(qs.len(), 2);
    assert_eq!(qs[0].formula(), "let\r\n    Source = 1+1\r\nin\r\n    Source");
    assert!(qs[0].meta.iter().any(|(k, v)| k == "RecoveryTargetSheet" && v == "sSheet2"));
    assert_eq!(qs[1].steps.len(), 1);
    let p = mashup::part_name(&back).unwrap();
    assert!(crate::xlsx::xml::part_text(&back.parts[&p]).contains("sqmid=\"7690c5d6"));
}

#[test]
fn queries_save_load_and_refresh_in_xlsx() {
    let mut wb = sales_book();
    let mut q = from_table_range(&mut wb, 0, parse_range("A1:D6").unwrap(), true).unwrap();
    assert_eq!(q.name, "Sales");
    assert_eq!(q.steps[0].op, Op::ChangeType(vec![("Region".into(), Ty::Text), ("Product".into(), Ty::Text), ("Units".into(), Ty::Int), ("Price".into(), Ty::Number)]));
    q.add_step(Op::FilterRows(cond_expr("Region", CondOp::NotEquals, &t("North"))), None);
    q.add_step(Op::RemoveColumns(vec!["Price".into()]), None);
    q.name = "Sales Query".into();
    let (si, ti) = close_and_load(&mut wb, &q, None, LoadTo::NewSheet, &no_web).unwrap().unwrap();
    assert_eq!(wb.sheets[si].name, "Sheet2");
    let tb = wb.sheets[si].tables[ti].clone();
    assert_eq!(tb.name, "Sales_Query");
    assert_eq!(tb.range, parse_range("A1:C5").unwrap());
    assert_eq!(wb.display(si, Pos::new(1, 2)), "10");
    assert_eq!(find(&wb, "Sales Query").unwrap().load, Load::Table("Sales_Query".into()));
    // The connection Excel lists and refreshes.
    let conns = crate::connections::list(&wb);
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].name, "Query - Sales Query");
    assert_eq!(conns[0].source, crate::connections::Source::Mashup { query: "Sales Query".into() });
    assert_eq!(crate::connections::destination(&wb, conns[0].id), Some((si, tb.range)));
    // Written as Excel writes it.
    let bytes = crate::xlsx::write(&wb).unwrap();
    let files = crate::xlsx::pkgcheck::unzip(&bytes);
    crate::xlsx::pkgcheck::check_package(&files);
    let c = &files["xl/connections.xml"];
    assert!(c.contains("type=\"5\"") && c.contains("Provider=Microsoft.Mashup.OleDb.1;Data Source=$Workbook$;Location=&quot;Sales Query&quot;;Extended Properties=&quot;&quot;") && c.contains("command=\"SELECT * FROM [Sales Query]\""), "{c}");
    let table = files.iter().find(|(k, v)| k.starts_with("xl/tables/table") && v.contains("Sales_Query")).unwrap();
    assert!(table.1.contains("tableType=\"queryTable\"") && table.1.contains("queryTableFieldId=\"3\""), "{}", table.1);
    let trels = &files[&crate::xlsx::rels_name_of(table.0)];
    assert!(trels.contains("relationships/queryTable"));
    let qt = files.iter().find(|(k, _)| k.starts_with("xl/queryTables/queryTable") && !k.contains("_rels")).unwrap().1;
    assert!(qt.contains("connectionId=\"1\"") && qt.contains("tableColumnId=\"3\"") && qt.contains("name=\"Units\""), "{qt}");
    assert!(files["xl/_rels/workbook.xml.rels"].contains("relationships/customXml\" Target=\"../customXml/item1.xml\""));
    assert!(files["customXml/_rels/item1.xml.rels"].contains("itemProps1.xml"));
    assert!(files["[Content_Types].xml"].contains("customXmlProperties+xml"));
    // Read back: editable queries, and a save that doesn't duplicate the query table.
    let mut back = crate::xlsx::read(&bytes).unwrap();
    let rq = find(&back, "Sales Query").unwrap();
    assert_eq!(rq.steps, q.steps);
    assert_eq!(rq.load, Load::Table("Sales_Query".into()));
    let again = crate::xlsx::pkgcheck::unzip(&crate::xlsx::write(&back).unwrap());
    assert_eq!(again.keys().filter(|k| k.starts_with("xl/queryTables/") && !k.contains("_rels")).count(), 1);
    // Refresh re-runs the source and steps.
    back.set_input(0, Pos::new(4, 0), "South");
    back.recalc();
    let id = crate::connections::list(&back)[0].id;
    let r = crate::connections::refresh(&mut back, id, &no_web).unwrap();
    assert_eq!(r, parse_range("A1:C6").unwrap());
    assert_eq!(back.display(si, Pos::new(5, 0)), "West");
    assert!(refresh_all(&mut back, &no_web).is_empty());
    // Renaming follows into the connection; deleting removes it.
    let mut rq = find(&back, "Sales Query").unwrap();
    rq.name = "Renamed".into();
    save(&mut back, &rq, Some("Sales Query")).unwrap();
    assert_eq!(crate::connections::list(&back)[0].source, crate::connections::Source::Mashup { query: "Renamed".into() });
    delete(&mut back, "Renamed").unwrap();
    assert!(crate::connections::list(&back).is_empty() && list(&back).is_empty() && mashup::part_name(&back).is_none());
    crate::xlsx::write(&back).unwrap();
}
