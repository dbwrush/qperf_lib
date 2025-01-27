use lazy_static::lazy_static;
use std::io::{self};
use std::fs;
use std::collections::HashMap;
use std::path::Path;

lazy_static! {
    static ref QUESTION_TYPE_INDICES: HashMap<char, usize> = {
        let mut m = HashMap::new();
        for (i, c) in ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].iter().enumerate() {
            m.insert(*c, i);
        }
        m
    };
}

pub fn get_question_types() -> Vec<char> {
    ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].to_vec()
}

pub fn qperformance(question_sets_dir_path: &str, quiz_data_path: &str) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    qperf(question_sets_dir_path, quiz_data_path, false, ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].to_vec(), ",".to_string())
}

pub fn qperf(question_sets_dir_path: &str, quiz_data_path: &str, verbose: bool, types: Vec<char>, delim: String) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    let mut warns = Vec::new();
    
    // Validate the paths
    if !Path::new(question_sets_dir_path).exists() {
        return Err(format!("Error: The path to the question sets does not exist: {}", question_sets_dir_path).into());
    }
    if !Path::new(quiz_data_path).exists() {
        return Err(format!("Error: The path to the quiz data does not exist: {}", quiz_data_path).into());
    }

    if verbose {
        //print requested question types
        eprintln!("Requested Question Types: {:?}", types);
    }

    //check that all chars in types are valid question types (from get_question_types())
    for c in &types {
        if !get_question_types().contains(c) {
            return Err(format!("Error: Invalid question type '{}'.", c).into());
        }
    }

    let mut entries = Vec::new();
    if Path::new(question_sets_dir_path).is_dir() {
        // Read the directory and sort the entries by name
        entries = fs::read_dir(question_sets_dir_path)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<_, io::Error>>()?;
        entries.sort();
        if verbose {//Display number of files found along with the path.
            eprintln!("Found {} files in directory: {:?}", entries.len(), question_sets_dir_path);            
        }
    } else if Path::new(question_sets_dir_path).is_file() {
        if verbose {
            eprintln!("Reading file: {:?}", question_sets_dir_path);
        }
        entries.push(Path::new(question_sets_dir_path).to_path_buf());
    } else {
        return Err(format!("Error: The path to the question sets is not a file or directory: {}", question_sets_dir_path).into());
    }

    //map round number to question types
    let mut question_types_by_round: HashMap<String, Vec<char>> = HashMap::new();

    for entry in entries {
        if let Some(ext) = entry.extension() {
            if ext == "rtf" {
                if verbose {
                    eprintln!("Found RTF file: {:?}", entry);
                }
                let question_types = read_rtf_file(entry.to_str().unwrap())?;
                //iterate through the map from this file and add to the main map, checking for duplicate round numbers and giving warnings for them.
                for (round_number, question_types) in question_types {
                    if question_types_by_round.contains_key(&round_number) {
                        eprintln!("Warning: Duplicate question set number: {}, using only the first.", round_number);
                    } else {
                        question_types_by_round.insert(round_number, question_types);
                    }
                }
            }
        }
    }
    if verbose {
        eprintln!("{:?}", question_types_by_round);
    }

    let mut quiz_records = vec![];
    //read quiz data file
    match read_csv_file(quiz_data_path) {
        Ok(records) => {
            quiz_records = records.clone();
        }
        Err(e) => eprintln!("Quiz data contains formatting error: {}", e),
    }

    let records = filter_records(quiz_records);
    if verbose {
        eprintln!("Found {} records", records.len());
    }
    let (quizzer_names, team_names) = get_quizzer_names(records.clone(), verbose, &mut warns);
    if verbose {
        eprintln!("Quizzer Names: {:?}", quizzer_names);
        eprintln!("Team Names: {:?}", team_names);
    }
    let num_quizzers = quizzer_names.len();
    let num_question_types = QUESTION_TYPE_INDICES.len();

    let mut attempts: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut correct_answers: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut bonus_attempts: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut bonus: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];

    update_arrays(&mut warns, records, &quizzer_names, question_types_by_round, &mut attempts, &mut correct_answers, &mut bonus_attempts, &mut bonus, false);

    let result = build_results(quizzer_names, attempts, correct_answers, bonus_attempts, bonus, types, delim, team_names);

    Ok((warns, result))
}

fn build_results(quizzer_names: Vec<String>, attempts: Vec<Vec<u32>>, correct_answers: Vec<Vec<u32>>, bonus_attempts: Vec<Vec<u32>>, bonus: Vec<Vec<u32>>, types: Vec<char>, delim: String, team_names: Vec<String>) -> String {
    let mut result = String::new();

    // Build the header
    result.push_str("Quizzer");
    result.push_str(&delim);
    result.push_str("Team");
    result.push_str(&delim);
    let mut question_types_list: Vec<_> = QUESTION_TYPE_INDICES.keys().collect();
    question_types_list.sort();
    for question_type in &question_types_list {
        if !types.contains(question_type) {
            continue;
        }
        result.push_str(&format!("{} Attempted{}{} Correct{}{} Bonuses Attempted{}{} Bonuses Correct{}", question_type, delim, question_type, delim, question_type, delim, question_type, delim));
    }
    result.push('\n');

    // Build the results for each quizzer
    for (i, quizzer_name) in quizzer_names.iter().enumerate() {
        //QuizMachine outputs often put single quotes around quizzer names. Check for them and remove them if present.
        let quizzer_name = quizzer_name.trim_matches('\'');
        let team_name = String::new();
        let team = team_names.get(i).unwrap_or(&team_name).trim_matches('\'');
        result.push_str(&format!("{}{}{}{}", quizzer_name, delim, team, delim));
        for question_type in &question_types_list {
            if types.len() > 0 && !types.contains(question_type) {
                continue;
            }
            let question_type_index = *QUESTION_TYPE_INDICES.get(question_type).unwrap_or(&0);
            result.push_str(&format!("{:.1}{}{:.1}{}{:.1}{}{:.1}{}",
                                     attempts[i][question_type_index], delim,
                                     correct_answers[i][question_type_index], delim,
                                     bonus_attempts[i][question_type_index], delim,
                                     bonus[i][question_type_index], delim));
        }
        result.push('\n');
    }

    result
}

fn update_arrays(warns: &mut Vec<String>, records: Vec<csv::StringRecord>, quizzer_names: &Vec<String>, question_types: HashMap<String, Vec<char>>, attempts: &mut Vec<Vec<u32>>, correct_answers: &mut Vec<Vec<u32>>, bonus_attempts: &mut Vec<Vec<u32>>, bonus: &mut Vec<Vec<u32>>, verbose: bool) {
    //list of skipped rounds
    let mut missing: Vec<String> = Vec::new();

    for record in records {

        // Split the record by commas to get the columns
        let columns: Vec<&str> = record.into_iter().collect();
        // Get the event type code, quizzer name, and question number
        let event_code = columns.get(10).unwrap_or(&"");

        let quizzer_name = columns.get(7).unwrap_or(&"");

        let round_number = columns.get(4).unwrap_or(&"");

        let question_number = columns.get(5).unwrap_or(&"").trim_matches('\'').parse::<usize>().unwrap_or(0) - 1;

        // Find the index of the quizzer in the quizzer_names array
        let quizzer_index = quizzer_names.iter().position(|n| n == quizzer_name).unwrap_or(0);

        // Check if the round is in the question types map
        if !question_types.contains_key(round_number as &str) {
            if !missing.contains(&round_number.to_string()) {
                missing.push(round_number.to_string());
            }
            //eprintln!("Warning: Skipping record due to missing question set for round {}", round_number);
            continue;
        }
        if verbose {
            eprintln!("{:?}", record);
        }
        if verbose {
            eprint!("ECode: {} ", event_code);
        }
        if verbose {
            eprint!("QName: {} ", quizzer_name);
        }
        if verbose {//print round number now in case it's invalid.
            eprint!("RNum: {} ", round_number);
        }
        if verbose {
            eprint!("QNum: {} ", question_number + 1);
        }
        // Get the question type based on question number
        let mut question_type = 'G';
        if (question_number + 1) != 21 {
            question_type = question_types.get(round_number as &str).unwrap_or(&vec!['G'])[question_number];
        }
        let question_type = question_type;

        //Q, R, and V all count towards a total for memory verses.
        let memory = question_type == 'Q' || question_type == 'R' || question_type == 'V';
        if verbose {
            eprintln!("QType: {} ", question_type);
        }
        // Find the index of the question type in the arrays
        let question_type_index = *QUESTION_TYPE_INDICES.get(&question_type).unwrap_or(&0);
        if verbose {
            eprintln!("QTInd: {} ", question_type_index);
        }
        // Update the arrays based on the event type code
        match *event_code {
            "'TC'" => {
                attempts[quizzer_index][question_type_index] += 1;
                correct_answers[quizzer_index][question_type_index] += 1;
                //also add for memory total
                if memory {
                    attempts[quizzer_index][8] += 1;
                    correct_answers[quizzer_index][8] += 1;
                }
            }
            "'TE'" => {
                attempts[quizzer_index][question_type_index] += 1;
                if memory {
                    attempts[quizzer_index][8] += 1;
                }
            }
            "'BC'" => {
                bonus_attempts[quizzer_index][question_type_index] += 1;
                bonus[quizzer_index][question_type_index] += 1;
                if memory {
                    bonus_attempts[quizzer_index][8] += 1;
                    bonus[quizzer_index][8] += 1;
                }
            }
            "'BE'" => {
                bonus_attempts[quizzer_index][question_type_index] += 1;
                if memory {
                    bonus_attempts[quizzer_index][8] += 1;
                }
            }
            _ => {}
        }
    }
    if missing.len() > 0 {
        //eprintln!("Warning: Some records were skipped due to missing question sets");
        warns.push("Warning: Some records were skipped due to missing question sets".to_string());
        //eprintln!("Skipped Rounds: {:?}", missing);
        warns.push(format!("Skipped Rounds: {:?}", missing));
        //Display the question set numbers found in the RTF files, sort them for easier reading.
        let mut found_rounds: Vec<_> = question_types.keys().collect();
        found_rounds.sort();
        eprintln!("Found Question Sets: {:?}", found_rounds);
        //eprintln!("If your question sets are not named correctly, please rename them to match the round numbers in the quiz data file");
        warns.push(format!("If your question sets are not named correctly, please rename them to match the round numbers in the quiz data file"));
    }
}

fn get_quizzer_names(records: Vec<csv::StringRecord>, verbose: bool, warns: &mut Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut current_team = String::new();
    let mut round_quizzers: Vec<String> = Vec::new();
    let mut round_teams: Vec<String> = Vec::new();
    let mut confirmed_quizzers: Vec<String> = Vec::new();
    let mut confirmed_teams: Vec<String> = Vec::new();
    let mut action = false;
    let mut index = 0;

    for record in records {
        /*
        So there's a really dumb problem here.
        Sometimes, the output from QuizMachine includes leftover team names from practice sessions.
        While I've never seen actual questions from these practice sessions show up, I HAVE seen the names appear.
        This can mean a quizzer's name appears in two teams (one from practice, one from the actual quiz).
        The below code is an attempt to remove practice team and quizzer names by only adding teams when they participate in 'action'
        */

        // Split the record by commas to get the columns
        let columns: Vec<&str> = record.into_iter().collect();
        let ecode = columns.get(10).unwrap_or(&"");//if this is "TN", it's a team name. If it's "QN", it's a quizzer name.
        let name = columns.get(7).unwrap_or(&"").to_string();//The name of either a quizzer or a team, depending on the event code.
        let team_number = columns.get(9).unwrap_or(&"").to_string();//team number from the current record. This gets reset to 0 at the start of each round.
        //If team_number becomes 0 before any action takes place, it means the names in round_teams might be from a practice session and can't be confirmed.
        if ecode == &"'TN'" {//team name. Check if they're already in the map, and add them if not.
            if team_number == "0" {//this is a new round.
                check_valid_round(&mut round_teams, &mut round_quizzers, &mut confirmed_teams, &mut confirmed_quizzers, verbose, &mut action);
            } else {
                if action {
                    //This shouldn't ever happen. But I've seen it happen. I'm honeslty not sure what should happen in this situation.
                    //So I figure we'll just run as normal, and give a warning.
                    warns.push(format!("Warning: Team  '{}' added mid-round at record #{}. This should not happen.", name, index));
                }
            }
            current_team = name.clone();
        } else if ecode == &"'QN'" {//quizzer name. Add to the team's list.
            if name != "''" && current_team != "''" {//if the name is not empty, add it to the list.
                round_quizzers.push(name.clone());
                round_teams.push(current_team.clone());
            }
        } else if ecode == &"'BC'" || ecode == &"'BE'" || ecode == &"'TC'" || ecode == &"'TE'" {//action has happened, teams present in this round can be confirmed.
            action = true;
        } else if ecode == &"'RM'" {//Indicates start of new round. Check if current teams can be confirmed.
            check_valid_round(&mut round_teams, &mut round_quizzers, &mut confirmed_teams, &mut confirmed_quizzers, verbose, &mut action);
        }

        index += 1;//shouldn't be needed, but for debugging why not have it?
    }

    if verbose {
        eprintln!("Confirmed Teams: {:?}", confirmed_teams);
        eprintln!("Confirmed Quizzers: {:?}", confirmed_quizzers);
    }

    (confirmed_quizzers, confirmed_teams)
}

fn check_valid_round(round_teams: &mut Vec<String>, round_quizzers: &mut Vec<String>, confirmed_teams: &mut Vec<String>, confirmed_quizzers: &mut Vec<String>, verbose: bool, action: &mut bool) {
    if *action {
        for i in 0..round_quizzers.len() {
            if !confirmed_quizzers.contains(&round_quizzers[i]) {
                confirmed_quizzers.push(round_quizzers[i].clone());
                confirmed_teams.push(round_teams[i].clone());
            }
        }
        if verbose {
            eprintln!("Confirming Teams: {:?}", round_teams);
            eprintln!("Confirming Quizzers: {:?}", round_quizzers);
        }
    } else {
        if verbose {
            eprintln!("No action taken in round, teams: {:?} might be from practice", round_teams);
        }
    }
    *action = false;
    round_teams.clear();
    round_quizzers.clear();
}

fn filter_records(records: Vec<csv::StringRecord>) -> Vec<csv::StringRecord> {
    let mut filtered_records = Vec::new();
    let event_codes = vec!["'TC'", "'TE'", "'BC'", "'BE'", "'TN'", "'QN'", "'RM'"]; // event type codes

    for record in records {
        // Split the record by commas to get the columns
        let columns: Vec<&str> = record.into_iter().collect();
        // Check if the 5th column matches the round number and 11th column contains the event type codes
        if columns.get(10).map_or(false, |v| event_codes.contains(&v)) {
            filtered_records.push(csv::StringRecord::from(columns));
        }
    }

    filtered_records
}

fn read_rtf_file(path: &str) -> io::Result<HashMap<String, Vec<char>>> {
    let content = fs::read_to_string(path)?;
    let re = regex::Regex::new(r"SET #([A-Za-z0-9]+)").unwrap();
    //println!("RTF Content:\n{}", content);
    let mut question_types = Vec::new();
    let mut question_types_by_round: HashMap<String, Vec<char>> = HashMap::new();
    let parts: Vec<_> = content.split("\\tab").collect();
    let mut round_number = String::new();
    for (i, part) in parts.iter().enumerate() {
        //Check if part contains a new set number. Check on every part in case there's weird formatting.
        match re.captures(&part) {
            Some(caps) => {
                if question_types.len() > 0 {// There are multiple question sets in this file, and we're not on the first one.
                    question_types_by_round.insert(round_number, question_types.clone());
                }
                round_number = format!("'{}'", caps.get(1).unwrap().as_str());
                question_types = Vec::new();
            },
            None => {}
        }
        
        if i % 2 == 0 && !part.is_empty() {
            //println!("{}", part);
            let chars: Vec<char> = part.chars().collect();
            let len = chars.len();
            if len > 1 {
                //print!("{}", chars[len - 2]);
                question_types.push(chars[len - 2]);
            }
        }
    }
    question_types_by_round.insert(round_number, question_types.clone());

    Ok(question_types_by_round)
}

fn read_csv_file(path: &str) -> Result<Vec<csv::StringRecord>, csv::Error> {
    let mut reader = csv::ReaderBuilder::new()
    .has_headers(false)
    .from_path(path)?;

    let mut records = Vec::new();

    for result in reader.records() {
        let record = result?;
        records.push(record);
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test for 'read_rtf_file' function
    #[test]
    fn test_read_rtf_file() {
        let sample_rtf_path = "tests/questions/sets.rtf"; // Ensure a sample file exists in `tests/`
        let result = read_rtf_file(sample_rtf_path);
        assert!(result.is_ok());
        let questions = result.unwrap();
        assert!(questions.len() > 0); // Validate that questions were parsed

        //assert_eq!(questions.len() == 1);
        //You may check the exact number by uncommenting the above line and setting the expected number of question sets in the file.
    }

    // Test for `read_csv_file` function
    #[test]
    fn test_read_csv_file() {
        let sample_csv_path = "tests/quiz_data.csv"; // Ensure a sample file exists in `tests/`
        let result = read_csv_file(sample_csv_path);
        assert!(result.is_ok());
        let records = result.unwrap();
        assert!(records.len() > 0); // Validate that records were read

        //assert_eq!(records.len() == 1);
        //You may check the exact number by uncommenting the above line and setting the expected number of records in the file.
    }

    // Test for `filter_records` function
    #[test]
    fn test_filter_records() {
        let filtered = filter_records(read_csv_file("tests/quiz_data.csv").unwrap());
        let expected = read_csv_file("tests/filtered_quiz_data.csv").unwrap();
        // Validate filtering logic (replace with actual expectations)
        assert_eq!(filtered.len(), expected.len());
    }
}