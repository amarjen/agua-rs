// This is like any another rust file
// every function we write here in the "pub mod create{}" and declared as public, can be invoked later by using "mod dbactions;"
pub mod create {
    use postgres::types::Type;
    use postgres::{Client, Error, NoTls}; // Used to map rust types to postgres types

    #[derive(Debug)] // This macro allows struct to be debugged
    struct Socio {
        id_socio: i16,
        nombre: String,
        iban: String,
        telefono: i64,
        email: String,
        id_forma_pago: i64,
        activo: bool,
        posicion: String,
        sector: String,
    }
    // Important to deserialize the data retrieved from the Database table
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

    pub fn get_socio(client: &mut Client, id_socio: i16) -> Result<String, Error> {
        let row = client.query_one("SELECT nombre FROM socios where id_socio=$1", &[&id_socio])?;

        Ok(row.get("nombre"))
    }

    pub fn get_iban(client: &mut Client, id_socio: i16) -> Result<String, Error> {
        let row = client.query_one("SELECT iban FROM socios WHERE id_socio=$1", &[&id_socio])?;

        Ok(row.get("iban"))
    }

    pub fn get_socios_activos(client: &mut Client) -> Result<Vec<i16>, Error> {
        let rows = client.query("SELECT id_socio FROM socios_activos;", &[])?;

        Ok(rows.into_iter().map(|row| row.get("id_socio")).collect())
    }

    pub fn get_derrama(client: &mut Client, id_periodo: &str) -> Result<i16, Error> {
        let row = client.query_one(
            "SELECT derrama from derramas where id_periodo=$1",
            &[&id_periodo],
        )?;
        let derrama: i64 = row.get(0);
        Ok(derrama as i16)
    }

    pub fn get_consumo_general(client: &mut Client, id_periodo: &str) -> Result<i16, Error> {
        let row = client.query_one(
            "SELECT m3 FROM consumos WHERE id_periodo=$1 and id_socio=0",
            &[&id_periodo],
        )?;
        let consumo: i64 = row.get(0);
        Ok(consumo as i16)
    }

    pub fn get_lecturas(
        client: &mut Client,
        id_periodo: &str,
        id_socio: i16,
    ) -> Result<[i16; 2], Error> {
        let id_periodo_anterior = periodo_anterior(id_periodo);

        let row = client.query_one(
            "select anterior, actual \
        from (select ant.m3 as anterior, act.m3 as actual \
        from (select id_socio, m3 from lecturas \
        where id_socio=$1 and id_periodo=$2 ) as ant \
        inner join \
        (select id_socio, m3 from lecturas \
        where id_socio=$1 and id_periodo=$3) as act \
        on ant.id_socio=act.id_socio) as lecturas;",
            &[&i64::from(id_socio), &id_periodo_anterior, &id_periodo],
        )?;

        let lectura_ant: i64 = row.get(0);
        let lectura_act: i64 = row.get(1);

        Ok([lectura_ant as i16, lectura_act as i16])
    }

    pub fn get_consumos(client: &mut Client, id_periodo: &str) -> Result<Vec<i64>, Error> {
        let id_periodo_anterior = periodo_anterior(id_periodo);
        let rows = client.query(
            "select id_socio, actual - anterior as consumo \
        from (select ant.id_socio, ant.m3 as anterior, act.m3 as actual \
        from (select id_socio, m3 from lecturas \
        where id_socio not in (0,100,200) and id_periodo=$1) as ant \
        inner join \
        (select id_socio, m3 from lecturas \
        where id_socio not in (0,100,200) and id_periodo=$2) as act \
        on ant.id_socio=act.id_socio) as consumos;",
            &[&id_periodo_anterior, &id_periodo],
        )?;
        Ok(rows.into_iter().map(|row| row.get(1)).collect())
    }

    pub fn get_socios_activos_count(client: &mut Client) -> Result<i64, Error> {
        let row = client.query_one("select count(id_socio) from socios_activos;", &[])?;
        Ok(row.get(0))
    }
}
