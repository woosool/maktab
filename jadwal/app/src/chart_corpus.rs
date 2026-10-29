//! Chart corpus probe: for every .xlsx in $JADWAL_CHART_DIR, prints each chart's model, renders
//! it to <name>-<sheet>-<n>.png in $JADWAL_CHART_OUT, saves the book as is and with every chart
//! regenerated (<name>.keep.xlsx / <name>.regen.xlsx), reopens both and compares the models.
//! `JADWAL_CHART_DIR=… JADWAL_CHART_OUT=… cargo test -p sheet-app chart_corpus -- --ignored --nocapture`

use crate::app::SheetApp;
use sheet_engine::chart::Chart;

fn charts(wb: &sheet_engine::Workbook) -> Vec<(usize, Chart)> {
    wb.sheets.iter().enumerate().flat_map(|(si, s)| s.objects.iter().filter_map(move |o| o.chart().map(|c| (si, c.clone())))).collect()
}

fn summary(wb: &sheet_engine::Workbook, si: usize, c: &Chart) -> String {
    let data = c.data(wb, si);
    let mut s = format!("kind={:?} grouping={:?} markers={} title={:?} legend={:?} labels={} grid={} x={:?} y={:?}\n", c.kind, c.grouping, c.markers, c.title, c.legend, c.data_labels, c.gridlines, c.x_title, c.y_title);
    for (se, d) in c.series.iter().zip(&data) {
        s += &format!("   ser kind={:?} name={:?} ({:?}) cat={:?} val={:?} -> cats={:?} vals={:?}\n", se.kind, d.name, se.name_ref, se.cat_ref, se.val_ref, d.cats.iter().take(4).collect::<Vec<_>>(), d.vals.iter().take(4).collect::<Vec<_>>());
    }
    s
}

fn model(c: &Chart) -> Chart { Chart { dirty: false, part: None, series: c.series.iter().map(|s| sheet_engine::chart::Series { cat_cache: vec![], val_cache: vec![], size_cache: vec![], color: None, name: if s.name_ref.is_some() { String::new() } else { s.name.clone() }, ..s.clone() }).collect(), ..c.clone() } }

#[test]
#[ignore]
fn chart_corpus() {
    let (Ok(dir), Ok(out)) = (std::env::var("JADWAL_CHART_DIR"), std::env::var("JADWAL_CHART_OUT")) else { return };
    std::fs::create_dir_all(&out).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "xlsx")).collect();
    files.sort();
    for f in files {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let wb = match sheet_engine::xlsx::read(&std::fs::read(&f).unwrap()) { Ok(w) => w, Err(e) => { println!("!! {stem}: read failed: {e:?}"); continue } };
        let cs = charts(&wb);
        println!("== {stem}: {} charts", cs.len());
        for (n, (si, c)) in cs.iter().enumerate() {
            print!(" [{si}:{n}] {}", summary(&wb, *si, c));
            let mut book = wb.clone();
            book.active = *si;
            let chart = c.clone();
            let mut app: Option<SheetApp> = None;
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(480.0, 300.0)).with_accessibility_check(false).build_ui(move |ui| {
                if app.is_none() { qalam_kit::fonts::install_fonts(ui.ctx()); let mut x = SheetApp::blank(ui.ctx().clone()); x.book = book.clone(); app = Some(x); return; }
                let r = ui.max_rect();
                crate::chart_ui::paint(app.as_ref().unwrap(), ui.painter(), &chart, r, 1.0);
            });
            h.run_steps(2);
            match h.render() { Ok(img) => img.save(format!("{out}/{stem}-{si}-{n}.png")).unwrap(), Err(e) => println!("   render failed: {e}") }
        }
        // Save as is, and regenerated.
        let keep = sheet_engine::xlsx::write(&wb).unwrap();
        std::fs::write(format!("{out}/{stem}.keep.xlsx"), &keep).unwrap();
        let mut regen = wb.clone();
        for s in &mut regen.sheets { for o in &mut s.objects { let _ = o.chart_mut(); } }
        let bytes = sheet_engine::xlsx::write(&regen).unwrap();
        std::fs::write(format!("{out}/{stem}.regen.xlsx"), &bytes).unwrap();
        for (label, b) in [("keep", keep), ("regen", bytes)] {
            let back = sheet_engine::xlsx::read(&b).unwrap();
            let again = charts(&back);
            if again.len() != cs.len() { println!("   !! {label}: {} charts after reopening, was {}", again.len(), cs.len()); continue; }
            for ((si, a), (_, b)) in cs.iter().zip(&again) {
                if model(a) != model(b) { println!("   !! {label} differs on sheet {si}:\n      before {}      after  {}", summary(&wb, *si, a), summary(&back, *si, b)); }
            }
        }
    }
}
