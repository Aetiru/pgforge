//! Un resultado como libro de Excel (`.xlsx`).
//!
//! Un `.xlsx` es un zip de unos pocos archivos XML. Armarlo a mano evita sumar una biblioteca de
//! hojas de cálculo entera para escribir una sola hoja: el zip va **sin comprimir** (método 0), que
//! Excel y LibreOffice abren igual, y lo único que hace falta de afuera es el `crc32` de cada entrada.
//!
//! Qué es número y qué es texto se decide por **columna**: una columna es numérica solo si todos sus
//! valores no nulos lo son, y un valor con ceros a la izquierda (`00123`), signo `+` o más de quince
//! cifras queda como texto, que es lo que hace que Excel no lo «arregle» en silencio. Las fechas van
//! como texto ISO tal como las entrega el servidor: convertirlas a número de serie exige saber el
//! tipo y la zona, y una fecha mal convertida es peor que una que se lee igual que en la base.
//!
//! Es puro y con tests: lo único que toca el disco es quien llama.

use crate::error::{Error, Result};

/// Filas máximas de una hoja de Excel, contando el encabezado.
const MAX_ROWS: usize = 1_048_576;
/// Caracteres máximos de una celda.
const MAX_CELL: usize = 32_767;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 no admite casi ningún carácter de control: un solo `\0` invalida el archivo.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Nombre de la columna: 0 → `A`, 25 → `Z`, 26 → `AA`.
fn column_letters(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

/// `true` si el texto se puede guardar como número sin perder nada de lo que se ve.
fn is_plain_number(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (int, frac) = body
        .split_once('.')
        .map_or((body, None), |(a, b)| (a, Some(b)));
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // `007` y `00.5` no: el cero de la izquierda es parte del dato.
    if int.len() > 1 && int.starts_with('0') {
        return false;
    }
    if let Some(frac) = frac {
        if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    // Excel guarda 15 cifras significativas: más que eso se redondea sin avisar.
    int.len() + frac.map_or(0, str::len) <= 15
}

fn zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();

    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(data);
        let size = data.len() as u32;

        // Cabecera local. Versión 20, sin banderas, método 0 (sin comprimir), fecha de DOS fija.
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x21u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        // Entrada del directorio central.
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }

    let central_offset = out.len() as u32;
    let central_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Resultado" sheetId="1" r:id="rId1"/></sheets></workbook>"#;

const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

/// Dos estilos: el 0 por omisión y el 1, negrita, para el encabezado.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs></styleSheet>"#;

/// El libro de una sola hoja con `columns` de encabezado y `rows` debajo. Los `None` son celdas
/// vacías: un NULL de la base no es una cadena vacía, y en la hoja se nota dejándolas sin valor.
pub fn workbook(columns: &[String], rows: &[Vec<Option<String>>]) -> Result<Vec<u8>> {
    if rows.len() + 1 > MAX_ROWS {
        return Err(Error::Config(format!(
            "Excel admite hasta {} filas por hoja y el resultado tiene {}; exportalo en CSV",
            MAX_ROWS - 1,
            rows.len()
        )));
    }
    if columns.len() > 16_384 {
        return Err(Error::Config("Excel admite hasta 16.384 columnas".into()));
    }

    let numeric: Vec<bool> = (0..columns.len())
        .map(|c| {
            let mut seen = false;
            for row in rows {
                match row.get(c).and_then(Option::as_deref) {
                    None => {}
                    Some(value) if is_plain_number(value) => seen = true,
                    Some(_) => return false,
                }
            }
            seen
        })
        .collect();

    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );

    sheet.push_str("<row r=\"1\">");
    for (c, name) in columns.iter().enumerate() {
        sheet.push_str(&format!(
            "<c r=\"{}1\" s=\"1\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            column_letters(c),
            escape(name)
        ));
    }
    sheet.push_str("</row>");

    for (r, row) in rows.iter().enumerate() {
        let number = r + 2;
        sheet.push_str(&format!("<row r=\"{number}\">"));
        for (c, is_number) in numeric.iter().enumerate() {
            let Some(value) = row.get(c).and_then(Option::as_deref) else {
                continue;
            };
            let reference = format!("{}{number}", column_letters(c));
            if *is_number {
                sheet.push_str(&format!("<c r=\"{reference}\"><v>{value}</v></c>"));
            } else {
                let value: String = value.chars().take(MAX_CELL).collect();
                sheet.push_str(&format!(
                    "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    escape(&value)
                ));
            }
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");

    Ok(zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes().to_vec()),
        ("_rels/.rels", ROOT_RELS.as_bytes().to_vec()),
        ("xl/workbook.xml", WORKBOOK.as_bytes().to_vec()),
        (
            "xl/_rels/workbook.xml.rels",
            WORKBOOK_RELS.as_bytes().to_vec(),
        ),
        ("xl/styles.xml", STYLES.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> String {
        // Sin compresión, el contenido de cada entrada está tal cual dentro del archivo.
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn las_letras_de_columna_siguen_el_orden_de_excel() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(27), "AB");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }

    #[test]
    fn solo_es_numero_lo_que_no_pierde_nada() {
        for ok in ["0", "12", "-5", "3.14", "-0.5", "999999999999999"] {
            assert!(is_plain_number(ok), "{ok}");
        }
        for no in [
            "00123",
            "+5",
            "1e5",
            "",
            "-",
            "1.",
            ".5",
            "1234567890123456",
            "0x10",
            "1,5",
            "007",
        ] {
            assert!(!is_plain_number(no), "{no}");
        }
    }

    #[test]
    fn escapa_xml_y_descarta_caracteres_de_control() {
        assert_eq!(escape("a<b>&\"c\u{0}\u{7}\n"), "a&lt;b&gt;&amp;&quot;c\n");
    }

    #[test]
    fn arma_un_zip_valido_con_las_seis_partes() {
        let bytes = workbook(
            &["id".into(), "nombre".into()],
            &[vec![Some("1".into()), Some("Ana".into())]],
        )
        .unwrap();
        // Firma local, firma de fin de directorio y las seis entradas.
        assert_eq!(&bytes[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let end = bytes.len() - 22;
        assert_eq!(&bytes[end..end + 4], &[0x50, 0x4b, 0x05, 0x06]);
        assert_eq!(u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]), 6);
        let all = text(&bytes);
        assert!(all.contains("xl/worksheets/sheet1.xml"));
        assert!(
            all.contains("<c r=\"A2\"><v>1</v></c>"),
            "la columna id es numérica"
        );
        assert!(all.contains("t=\"inlineStr\"><is><t xml:space=\"preserve\">Ana</t>"));
    }

    #[test]
    fn una_columna_con_ceros_a_la_izquierda_queda_como_texto() {
        let rows = vec![vec![Some("00123".into())], vec![Some("7".into())]];
        let all = text(&workbook(&["codigo".into()], &rows).unwrap());
        assert!(all.contains("<t xml:space=\"preserve\">00123</t>"));
        assert!(
            !all.contains("<v>7</v>"),
            "una columna mezclada es de texto entera"
        );
    }

    #[test]
    fn un_null_deja_la_celda_vacia() {
        let rows = vec![vec![Some("1".into()), None], vec![None, Some("x".into())]];
        let all = text(&workbook(&["a".into(), "b".into()], &rows).unwrap());
        assert!(!all.contains("r=\"B2\""));
        assert!(!all.contains("r=\"A3\""));
        assert!(all.contains("r=\"B3\""));
    }

    #[test]
    fn el_crc_de_cada_entrada_coincide() {
        let bytes = workbook(&["a".into()], &[]).unwrap();
        // La primera entrada es `[Content_Types].xml`: su CRC está en el offset 14 de la cabecera.
        let crc = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
        assert_eq!(crc, crc32fast::hash(CONTENT_TYPES.as_bytes()));
    }

    #[test]
    fn demasiadas_filas_se_rechazan_con_un_motivo() {
        let rows = vec![vec![None::<String>]; MAX_ROWS];
        assert!(workbook(&["a".into()], &rows).is_err());
    }
}
