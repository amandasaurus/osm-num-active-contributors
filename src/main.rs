use anyhow::Result;
use chrono::naive::NaiveDate;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use osmio::prelude::*;
use osmio::OSMObjBase;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// OSM History file to read.
    #[arg(short, long)]
    input_filename: PathBuf,

    /// All output files will be prefixed with this string.
    #[arg(short = 'p', long, default_value = "")]
    output_prefix: String,

    /// Output only includes entries for people who have mapped at least this many days. default: 0
    ///
    /// Often a large number of mappers have 1 or 2 edit days, which clutters the data.
    #[arg(long, default_value = "20")]
    min_edit_days: u32,

    /// When producing per-day stats, start on this first day. Default is to start from the
    /// earliest day in the history file.
    #[arg(long)]
    start_date: Option<NaiveDate>,

    /// When producing per-day stats, produce stats up to this date. Default is today.
    #[arg(long)]
    end_date: Option<NaiveDate>,

    /// When producing per-day stats, include at least this many days in the output.
    #[arg(long, default_value = "3")]
    min_num_days: Option<u32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let input_file = File::open(args.input_filename)?;
    let input_bar = ProgressBar::new(input_file.metadata()?.len());
    let mut reader = osmio::pbf::PBFReader::new(input_bar.wrap_read(input_file));
    input_bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {eta} {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap(),
    );

    let (user_edit_days, day_edit_users, last_username): (
        HashMap<u32, BTreeSet<NaiveDate>>,
        BTreeMap<NaiveDate, HashSet<u32>>,
        HashMap<u32, (i64, String)>,
    ) = reader
        .objects()
        .par_bridge()
        .fold(
            Default::default,
            |(mut user_edit_days, mut day_edit_users, mut last_username): (
                HashMap<u32, BTreeSet<NaiveDate>>,
                BTreeMap<NaiveDate, HashSet<u32>>,
                HashMap<u32, (i64, String)>,
            ),
             o| {
                let timestamp = o.timestamp().as_ref().unwrap().to_epoch_number();
                let day_string = o.timestamp().as_ref().unwrap().to_iso_string();
                let day = NaiveDate::from_ymd_opt(
                    day_string.get(0..4).unwrap().parse().unwrap(),
                    day_string.get(5..7).unwrap().parse().unwrap(),
                    day_string.get(8..10).unwrap().parse().unwrap(),
                )
                .unwrap();
                let uid = o.uid().unwrap();
                if last_username
                    .get(&uid)
                    .map_or(true, |(ts, un)| ts <= &timestamp && un != o.user().unwrap())
                {
                    last_username.insert(uid, (timestamp, o.user().unwrap().to_owned()));
                }

                user_edit_days.entry(uid).or_default().insert(day);
                day_edit_users.entry(day).or_default().insert(uid);
                (user_edit_days, day_edit_users, last_username)
            },
        )
        .reduce_with(
            |(mut user_edit_days1, mut day_edit_users1, mut last_username1),
             (mut user_edit_days2, day_edit_users2, mut last_username2)| {
                for (uid, days) in user_edit_days2.drain() {
                    user_edit_days1.entry(uid).or_default().extend(days);
                }
                for (day, uids) in day_edit_users2.into_iter() {
                    day_edit_users1.entry(day).or_default().extend(uids);
                }
                for (uid, (ts2, un2)) in last_username2.drain() {
                    if last_username1
                        .get(&uid)
                        .map_or(true, |(ts1, un1)| &ts2 >= ts1 && &un2 != un1)
                    {
                        last_username1.insert(uid, (ts2, un2));
                    }
                }
                (user_edit_days1, day_edit_users1, last_username1)
            },
        )
        .unwrap();

    input_bar.finish();
    println!(
        "All data read in. Have {} users & {} days",
        user_edit_days.len(),
        day_edit_users.len()
    );

    let input_day_range = (
        day_edit_users.first_key_value().unwrap().0,
        day_edit_users.last_key_value().unwrap().0,
    );

    let mut output_per_day = csv::Writer::from_writer(BufWriter::new(File::create(format!(
        "{}user_totals_per_day.csv",
        args.output_prefix
    ))?));
    let mut output_date_per_uid = csv::Writer::from_writer(BufWriter::new(File::create(
        format!("{}users_per_day.csv", args.output_prefix).to_string(),
    )?));

    output_per_day.write_record(["date", "num_users", "rolling_yr_total", "users_ge42_days"])?;
    let year = chrono::Days::new(365);
    for day in input_day_range
        .0
        .iter_days()
        .take_while(|d| d <= input_day_range.1)
    {
        let date_str = day.format("%F").to_string();
        let total_num_users = day_edit_users
            .get(&day)
            .map_or("0".to_string(), |uids| uids.len().to_string());
        // kinda repeating users_per_day but for last year
        let uids_last_year: HashMap<u32, HashSet<&NaiveDate>> = day_edit_users
            .range(day - year..=day)
            .flat_map(move |(this_day, uids)| uids.iter().map(move |uid| (uid, this_day)))
            .fold(HashMap::new(), |mut user_totals, (uid, day)| {
                user_totals.entry(*uid).or_default().insert(day);
                user_totals
            });
        output_per_day.write_record(&[
            date_str,
            total_num_users,
            uids_last_year.len().to_string(),
            uids_last_year
                .values()
                .filter(|days| days.len() >= 42)
                .count()
                .to_string(),
        ])?;
    }

    output_date_per_uid.write_record([
        "date",
        "uid",
        "num_edit_days_last_yr",
        "username",
        "ge42days",
        "mapped_days",
    ])?;
    let start_date = args.start_date.unwrap_or(input_day_range.0.clone());
    let end_date = args.end_date.unwrap_or(input_day_range.1.clone());
    let mut start_date = clamp(
        start_date,
        input_day_range.0.clone(),
        input_day_range.1.clone(),
    );
    let end_date = clamp(
        end_date,
        input_day_range.0.clone(),
        input_day_range.1.clone(),
    );
    if let Some(min_num_days) = args.min_num_days {
        if end_date - start_date < chrono::TimeDelta::try_days(min_num_days.into()).unwrap() {
            start_date = start_date - chrono::Days::new(min_num_days.into());
        }
    }
    for specific_date in start_date.iter_days().take_while(|d| d <= &end_date) {
        let users_days: BTreeMap<u32, BTreeSet<&NaiveDate>> = day_edit_users
            .range(specific_date - year..=specific_date)
            .flat_map(|(this_day, uids)| uids.iter().map(move |uid| (uid, this_day)))
            .fold(Default::default(), |mut user_totals, (uid, day)| {
                user_totals.entry(*uid).or_default().insert(day);
                user_totals
            });
        let specific_date_str = specific_date.format("%F").to_string();
        for (uid, days) in users_days.iter() {
            if days.len() >= args.min_edit_days as usize {
                output_date_per_uid.write_record([
                    specific_date_str.as_str(),
                    &uid.to_string(),
                    &days.len().to_string(),
                    &last_username.get(uid).unwrap().1,
                    if days.len() >= 42 { "yes" } else { "no" },
                    &days
                        .iter()
                        .map(|d| d.format("%d.%m.").to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ])?;
            }
        }
    }

    println!("Finished");
    Ok(())
}

fn clamp<T: Ord>(val: T, min_val: T, max_val: T) -> T {
    if val > max_val {
        max_val
    } else if val < min_val {
        min_val
    } else {
        val
    }
}
