use crossterm::terminal;
use crossterm::{
    execute,
    style::{Attribute, Color, Print, SetAttribute, SetForegroundColor},
};
use crossterm::tty::IsTty;
use std::fs::File;
use std::io;
use std::io::{Error, Write};
use std::path::PathBuf;

fn write_color<W: Write + IsTty>(writer: &mut W, r: u8, g: u8, b: u8, message: &str) {
    if writer.is_tty() {
        execute!(
            writer,
            SetForegroundColor(Color::Rgb { r, g, b }),
            // SetAttribute(Attribute::Bold),
            Print(message),
            SetAttribute(Attribute::Reset)
        )
        .unwrap();
    } else {
        writeln!(writer, "{}", message).unwrap();
    }
}

#[derive(PartialEq)]
pub(crate) enum RowableContext {
    Delimited,
    TTY,
    // Undefined, // not delimited or tty
}

/// Translate one struct into one or more rows (Vec<String>). Note that the number of resultant columns not be equal to the number of struct fields.
pub(crate) trait Rowable {
    fn to_rows(&self, context: &RowableContext) -> Vec<Vec<String>>;
}

fn to_writer_delimited<W: Write>(
    writer: &mut W,
    row: &[String],
    delimiter: &str,
) -> Result<(), Error> {
    let row_str = row.join(delimiter);
    writeln!(writer, "{}", row_str)?;
    Ok(())
}

#[derive(Debug)]
struct WidthFormat {
    width_pad: usize,
    width_chars: usize,
}

fn optimize_widths(
    widths_max: &Vec<usize>,
    ellipsisable: &Vec<bool>,
    w_gutter: usize,
) -> Vec<WidthFormat> {
    // total characters needed; we add a gutter after all columns, even the last one
    let w_total: usize = widths_max.iter().sum::<usize>() + (w_gutter * widths_max.len());
    let ellipsisable_any = ellipsisable.iter().any(|&x| x);
    let w_terminal = match terminal::size() {
        Ok((w, _)) => w,
        _ => 0,
    };

    if !ellipsisable_any || w_total <= w_terminal.into() || w_terminal == 0 {
        return widths_max
            .iter()
            .map(|e| WidthFormat {
                width_chars: *e,
                width_pad: *e + w_gutter,
            })
            .collect();
    }
    let w_excess: f64 = (w_total - w_terminal as usize) as f64; // width to trim
    let mut widths = Vec::new();

    let w_ellipsisable: usize = widths_max
        .iter()
        .zip(ellipsisable.iter())
        .filter(|(_, &is_ellipsisable)| is_ellipsisable)
        .map(|(width, _)| width)
        .sum();

    for (i, width) in widths_max.iter().enumerate() {
        if ellipsisable[i] {
            let proportion = *width as f64 / w_ellipsisable as f64;
            let reduction = (proportion * w_excess) as usize;
            let w_field = (*width - reduction).max(3);
            widths.push(WidthFormat {
                width_chars: w_field - w_gutter,
                width_pad: w_field,
            })
        } else {
            widths.push(WidthFormat {
                width_chars: *width,
                width_pad: width + w_gutter,
            });
        }
    }
    // proportional reduction from all
    // for width in widths_max.iter() {
    //     let proportion = *width as f64 / w_total as f64;
    //     let reduction = (proportion * w_excess) as usize;
    //     let w_field = (*width - reduction).max(3);
    //     widths.push(WidthFormat {
    //         width_chars: w_field - w_gutter,
    //         width_pad: w_field,
    //     });
    // }
    widths
}

fn prepare_field(value: &String, widths: &WidthFormat) -> String {
    if value.len() <= widths.width_chars {
        format!("{:<w$}", value, w = widths.width_pad)
    } else {
        if widths.width_chars > 3 && (value.len() - widths.width_chars) > 3 {
            format!(
                "{:<w$}",
                format!("{}...", &value[..(widths.width_chars - 3)]),
                w = widths.width_pad
            )
        } else {
            format!("{:<w$}", &value[..widths.width_chars], w = widths.width_pad)
        }
    }
}

/// Wite Rowables to a writer. If `delimiter` is None, we assume writing to stdout; if `delimiter` is not None, we assume writing a delimited text file.
fn to_table_writer<W: Write, T: Rowable>(
    writer: &mut W,
    headers: Vec<HeaderFormat>,
    records: &Vec<T>,
    delimiter: Option<&str>,
    context: RowableContext,
) -> Result<(), Error> {
    if records.is_empty() || headers.is_empty() {
        return Ok(());
    }
    let header_labels: Vec<String> = headers.iter().map(|hf| hf.header.clone()).collect();
    let ellipsisable: Vec<bool> = headers.iter().map(|hf| hf.ellipsisable).collect();

    match delimiter {
        Some(delim) => {
            to_writer_delimited(writer, &header_labels, delim)?;
            for record in records {
                for row in record.to_rows(&context) {
                    to_writer_delimited(writer, &row, delim)?;
                }
            }
        }
        None => {
            // evaluate headers and all elements in every row to determine max colum widths; store extracted rows for reuse in writing body.
            let mut widths_max = vec![0; headers.len()];
            for (i, header) in header_labels.iter().enumerate() {
                widths_max[i] = header.len();
            }
            let mut rows = Vec::new();
            for record in records {
                for row in record.to_rows(&context) {
                    for (i, element) in row.iter().enumerate() {
                        widths_max[i] = widths_max[i].max(element.len());
                    }
                    rows.push(row);
                }
            }
            let w_gutter = 2;
            let widths = optimize_widths(&widths_max, &ellipsisable, w_gutter);
            // header
            for (i, header) in header_labels.into_iter().enumerate() {
                write!(writer, "{}", prepare_field(&header, &widths[i]),)?;
            }
            writeln!(writer)?;
            // body
            for row in rows {
                for (i, element) in row.into_iter().enumerate() {
                    write!(writer, "{}", prepare_field(&element, &widths[i]),)?;
                }
                writeln!(writer)?;
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct HeaderFormat {
    header: String,
    ellipsisable: bool,
}

impl HeaderFormat {
    pub(crate) fn new(header: String, ellipsisable: bool) -> HeaderFormat {
        HeaderFormat {
            header,
            ellipsisable,
        }
    }
}

pub(crate) trait Tableable<T: Rowable> {
    fn get_header(&self) -> Vec<HeaderFormat>;
    fn get_records(&self) -> &Vec<T>;

    fn to_writer<W: Write>(
        &self,
        mut writer: W,
        delimiter: Option<&str>,
        context: RowableContext,
    ) -> io::Result<()> {
        let _ = to_table_writer(
            &mut writer,
            self.get_header(),
            self.get_records(),
            delimiter,
            context,
        );
        Ok(())
    }

    fn to_file(&self, file_path: &PathBuf, delimiter: char) -> io::Result<()> {
        let file = File::create(file_path)?;
        self.to_writer(
            file,
            Some(&delimiter.to_string()),
            RowableContext::Delimited,
        )
    }

    fn to_stdout(&self) -> io::Result<()> {
        let stdout = io::stdout();
        let handle = stdout.lock();
        // TODO: check if we are a TTY
        self.to_writer(handle, None, RowableContext::TTY)
    }
}
