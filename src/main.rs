//! Genera recibos y remesa para la Junta del Agua.
use calamine::{open_workbook, Ods, Reader};
use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Confirm};
use excel::Row;
use serde::Serialize;
use simple_excel_writer as excel;
use std::process;
use tabled::{object::Columns, Alignment, Modify, Style, Table, Tabled};
use tera::{Context, Tera};

use configparser::ini::Ini;
use postgres::{Client, NoTls};
use std::error::Error;
mod dbactions;
use dbactions::create::*;

mod cdp;
use cdp::Contador;

#[derive(Parser)]
#[command(name = "Recibos de Agua Casas del Pino")]
#[command(author = "Tony MJ. <tonymj@pm.me>")]
#[command(version = "1.0")]
#[command(about = "Genera recibos y remesa para la Junta de Agua", long_about = None)]

struct Cli {
    #[arg(long)]
    periodo: String,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Lectura {
    socio: i16,
    periodo: String,
    lectura: i16,
}

#[derive(Debug, Serialize)]
struct Concepto {
    nombre: String,
    importe: f32,
}

/// Representa un "Recibo"
#[derive(Debug)]
struct Recibo {
    periodo: String,
    socio: i16,
    nombre: String,
    iban: String,
    lecturas: [i16; 2],
    consumo: i16,
    consumo_bloques: [i16; 4],
    importe_bloques: [f32; 4],
    importe: f32,
    derrama: f32,
    conceptos: [Concepto; 8],
    total: f32,
}

impl Recibo {
    /// Cada nuevo recibo se genera para cada socio, en cada periodo, en base a
    /// una lectura.
    ///
    fn new(
        client: &mut Client,
        periodo: &str,
        socio: i16,
        derrama: f32,
    ) -> Result<Recibo, Box<dyn Error>> {
        let lecturas: [i16; 2] = get_lecturas(client, periodo, socio)?;
        let consumo = lecturas[1] - lecturas[0];

        let r = Recibo {
            periodo: String::from(periodo),
            socio: socio,
            nombre: get_socio(client, socio)?,
            iban: get_iban(client, socio)?,
            lecturas: lecturas,
            consumo: consumo,
            consumo_bloques: consumo_por_bloques(consumo, &Contador::Usuario),
            importe_bloques: importe_por_bloques(consumo, &Contador::Usuario),
            conceptos: [
                Concepto {
                    nombre: "Cuota de servicio".to_string(),
                    importe: 14.36,
                },
                Concepto {
                    nombre: "Conservación Contador".to_string(),
                    importe: 1.67,
                },
                Concepto {
                    nombre: "Basura".to_string(),
                    importe: 10.80,
                },
                Concepto {
                    nombre: "Supervisión de contadores".to_string(),
                    importe: 6.00,
                },
                Concepto {
                    nombre: "Cuota de recibo".to_string(),
                    importe: 0.0,
                },
                Concepto {
                    nombre: "Cuota de mantenimiento".to_string(),
                    importe: 10.0,
                },
                Concepto {
                    nombre: "Ajuste".to_string(),
                    importe: 0.0,
                },
                Concepto {
                    nombre: "Derrama".to_string(),
                    importe: 0.0,
                },
            ],
            importe: importe(consumo, &Contador::Usuario),
            derrama: derrama,
            total: 0.00,
        };
        Ok(r.calcular_total())
    }

    fn calcular_total(mut self) -> Recibo {
        let c: f32 = self.conceptos.iter().map(|c| c.importe).sum();
        self.total = self.importe + self.derrama + c;
        self
    }

    /// Transforma un Recibo en formato para remesa.
    fn to_remesa(recibo: &Recibo) -> Remesa {
        Remesa {
            socio: f64::from(recibo.socio),
            iban: String::from(&recibo.iban),
            nombre: String::from(&recibo.nombre),
            total: f64::from(recibo.total),
        }
    }

    fn to_filas(recibo: &Recibo) -> Filas {
        Filas {
            socio: i16::from(recibo.socio),
            nombre: recibo.nombre.to_string(),
            anterior: format!("{:.2}", recibo.lecturas[0]),
            actual: format!("{:.2}", recibo.lecturas[1]),
            m3: format!("{:.2}", recibo.consumo),
            derrama: format!("{:.2}", recibo.derrama),
            importe: format!("{:.2}", recibo.importe),
            total: format!("{:.2}", recibo.total),
        }
    }
}

fn Continue() -> () {
    match Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("¿Continuar?")
        .interact_opt()
        .unwrap()
    {
        Some(true) => (),
        Some(false) => process::exit(0),
        None => (),
    }
}

/// Representa los datos de un recibo para enviar por remesa bancaria.
/// en formato excel.
struct Remesa {
    socio: f64,
    iban: String,
    //fecha_mandato: String,
    //...
    nombre: String,
    total: f64,
}

#[derive(Tabled)]
struct Filas {
    socio: i16,
    nombre: String,
    anterior: String,
    actual: String,
    m3: String,
    importe: String,
    derrama: String,
    total: String,
}

fn leer_ods(periodo: &str) -> Vec<Lectura> {
    let (n_periodo, year) = parse_periodo(&periodo);

    let month = match n_periodo {
        1 => 1,
        2 => 3,
        3 => 5,
        4 => 7,
        5 => 9,
        6 => 11,
        _ => panic!("Error, mes no corresponde a ningun periodo!!!"),
    };

    let mut excel: Ods<_> = open_workbook(format!("lectura{year}.ods")).unwrap();
    let mut v: Vec<Lectura> = Vec::new();

    if let Some(Ok(r)) = excel.worksheet_range(&format!("{month:02}{year}")) {
        for row in r.rows() {
            if row[0].is_float() && row[4].is_float() {
                v.push(Lectura {
                    socio: row[0].get_float().unwrap() as i16,
                    periodo: String::from(periodo),
                    lectura: row[4].get_float().unwrap() as i16,
                });
            }
        }
    }
    v
}

fn consumo_por_bloques(consumo: i16, calculo: &Contador) -> [i16; 4] {
    match calculo {
        Contador::Usuario => match consumo {
            0..=9 => [consumo, 0, 0, 0],
            10..=27 => [9, consumo - 9, 0, 0],
            28..=80 => [9, 18, consumo - 27, 0],
            _ => [9, 18, 53, consumo - 80],
        },
        Contador::General => match consumo {
            0..=423 => [consumo, 0, 0, 0],
            424..=1269 => [423, consumo - 423, 0, 0],
            1270..=3760 => [423, 846, consumo - 1269, 0],
            _ => [423, 864, 2491, consumo - 3760],
        },
    }
}

fn importe_por_bloques(consumo: i16, calculo: &Contador) -> [f32; 4] {
    let tarifas = [0.5421, 1.449_535, 1.944_467, 2.8033];

    let consumos = match calculo {
        Contador::Usuario => consumo_por_bloques(consumo, &Contador::Usuario),
        Contador::General => consumo_por_bloques(consumo, &Contador::General),
    };

    let mut importes = [0.0; 4];
    for i in 0..4 {
        importes[i] = (100.0 * tarifas[i] * 1.1 * f32::from(consumos[i])).round() / 100.0;
    }
    importes
}

fn importe(consumo: i16, calculo: &Contador) -> f32 {
    match calculo {
        Contador::Usuario => importe_por_bloques(consumo, &Contador::Usuario)
            .iter()
            .sum(),
        Contador::General => importe_por_bloques(consumo, &Contador::General)
            .iter()
            .sum(),
    }
}

/// Importe total de los socios desde un vector de Recibos
fn get_importe_total_socios(recibos: &[Recibo]) -> f32 {
    recibos
        .iter()
        .map(|recibo| recibo.importe)
        .collect::<Vec<f32>>()
        .iter()
        .sum()
}

fn parse_periodo(id_periodo: &str) -> (i32, i32) {
    let v: Vec<&str> = id_periodo.split('-').collect();
    let year: i32 = v[0].parse::<i32>().unwrap();
    let n_periodo: i32 = v[1].parse::<i32>().unwrap();
    (n_periodo, year)
}

fn periodo_anterior(id_periodo: &str) -> String {
    let v: Vec<&str> = id_periodo.split('-').collect();
    let year: i32 = v[0].parse::<i32>().unwrap();
    let n_periodo: i32 = v[1].parse::<i32>().unwrap();
    match n_periodo {
        1 => format!("{}-6", (year - 1)),
        2..=6 => format!("{}-{}", year, (n_periodo - 1)),
        _ => panic!["Periodo incorrecto. Solo es válido del 1 al 6."],
    }
}

fn get_derrama_por_socio(client: &mut Client, id_periodo: &str) -> Result<f32, Box<dyn Error>> {
    let fra_hidrogea = importe(get_consumo_general(client, id_periodo)?, &Contador::General);

    let mut consumos: Vec<f32> = Vec::new();

    for consumo in get_consumos(client, &id_periodo)?.into_iter() {
        let i = importe(consumo as i16, &Contador::Usuario);
        consumos.push(i);
    }

    let total_socios: f32 = consumos.iter().sum();
    let derrama_importe = fra_hidrogea - total_socios;
    let num_socios: i64 = get_socios_activos_count(client)?;

    Ok(derrama_importe / num_socios as f32)
}

fn genera_remesa_excel(recibos: &Vec<Recibo>, fichero: &str) {
    let mut wb = excel::Workbook::create(fichero);
    let mut sheet = wb.create_sheet("remesa");

    sheet.add_column(excel::Column { width: 5.0 });
    sheet.add_column(excel::Column { width: 35.0 });
    sheet.add_column(excel::Column { width: 35.0 });
    sheet.add_column(excel::Column { width: 10.0 });

    wb.write_sheet(&mut sheet, |sheet_writer| {
        let sw = sheet_writer;
        sw.append_row(excel::row!["Socio", "Nombre", "IBAN", "Total"])?;
        for recibo in recibos {
            let remesa: Remesa = Recibo::to_remesa(recibo);
            sw.append_row(excel::row![
                remesa.socio,
                remesa.nombre,
                remesa.iban,
                remesa.total
            ])?;
        }
        Ok(())
    })
    .expect("write excel error!!");
    wb.close().expect("close excel error!");
}

fn genera_recibos_md(recibos: &Vec<Recibo>) {
    let tera = match Tera::parse("templates/**/*") {
        Ok(t) => t,
        Err(e) => {
            println!("Parsing error(s): {e}");
            ::std::process::exit(1);
        }
    };

    for recibo in recibos {
        let mut context = Context::new();

        context.insert("periodo", &recibo.periodo);
        context.insert("nombre", &recibo.nombre);
        context.insert("anterior", &recibo.lecturas[0]);
        context.insert("actual", &recibo.lecturas[1]);
        context.insert("m3", &recibo.consumo);
        context.insert("eur", &recibo.importe);
        context.insert("importes", &recibo.importe_bloques);
        context.insert("consumo_bloques", &recibo.consumo_bloques);
        context.insert("conceptos", &recibo.conceptos);

        // let mut context = Context::from_serialize(&recibo.conceptos);

        let rendered = tera
            .render("plantilla.md", &context)
            .expect("Failed to render template");
        println!("{rendered}");
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Archivo de configuración con datos de conexión a la bd
    let mut config = Ini::new();
    let _configmap = config.load("config.ini")?;
    let uri = &format!(
        "postgresql://{user}:{password}@{host}:{port}/{database}",
        user = config.get("db", "user").unwrap(),
        password = config.get("db", "pw").unwrap(),
        host = config.get("db", "host").unwrap(),
        port = config.get("db", "port").unwrap(),
        database = config.get("db", "database").unwrap(),
    );

    let cli = Cli::parse();

    // Conexion con base de datos sqlite
    //    let client.= Connection::open("junta.sqlite")?;
    let mut client = Client::connect(uri, NoTls)?;

    // Leer lecturas de fichero ODS
    let _lecturas = leer_ods(&cli.periodo);

    // TODO:
    // en este punto hay que añadir las lecturas a la base de datos
    //insert_lecturas_bbdd(&client. &lecturas).expect("Error de bbdd");

    // TODO:
    // Comprobar que todos los socios activos tienen lectura.

    let mut recibos: Vec<Recibo> = Vec::new();

    let derrama_por_socio: f32 = get_derrama_por_socio(&mut client, &cli.periodo)?;
    for socio in get_socios_activos(&mut client)? {
        recibos.push(Recibo::new(
            &mut client,
            &cli.periodo,
            socio,
            derrama_por_socio,
        )?)
    }

    // Imprimir tabla de datos
    // TODO: escribir una fn que imprima la tabla.
    let mut rows: Vec<Filas> = Vec::new();
    for recibo in &recibos {
        rows.push(Recibo::to_filas(recibo));
    }
    rows.sort_by_key(|row| row.socio);

    let table = Table::new(rows)
        .with(Alignment::right())
        .with(Modify::new(Columns::single(1)).with(Alignment::left()))
        .with(Style::rounded())
        .to_string();

    println!("{table}");

    // Generar Recibos .md
    // TODO: Completar plantilla y cambiar output a ficheros
    genera_recibos_md(&recibos);

    // derrama

    let fra_hidrogea = importe(
        get_consumo_general(&mut client, &cli.periodo)?,
        &Contador::General,
    );

    let total_socios: f32 = get_importe_total_socios(&recibos);
    let derrama = get_derrama(&mut client, &cli.periodo)?;
    let derrama_importe = fra_hidrogea - total_socios;
    let derrama_por_usuario = derrama_importe / 45.0;

    println!("Factura Hidrogea: {fra_hidrogea:?}");
    println!("Facturas Socios: {total_socios:?}");
    println!("Derrama: {derrama:?} m3");
    println!("Derrama importe: {derrama_importe:?} eur , {derrama_por_usuario}");

    match Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("¿Continuar con la generación de archivos? (plots, recibos, remesa)")
        .interact_opt()
        .unwrap()
    {
        Some(false) => process::exit(0),
        _ => (),
    }

    genera_remesa_excel(&recibos, "test.xlsx");
    Ok(())
}
