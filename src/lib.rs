use lazy_static::lazy_static;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self};
use std::fs;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

lazy_static! {
    static ref QUESTION_TYPE_INDICES: HashMap<char, usize> = {
        let mut m = HashMap::new();
        for (i, c) in ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].iter().enumerate() {
            m.insert(*c, i);
        }
        m
    };
}

//struct representing a round of quizzing, containing the names and scores for 2-3 teams.
pub struct Round {
    pub round_number: String,
    pub room_number : String,
    pub team_names  : Vec<String>,
    pub team_scores : Vec<i32>,
}

pub fn get_question_types() -> Vec<char> {
    ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].to_vec()
}

pub fn qperformance(question_sets_path: &str, quiz_data_path: &str) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    qperf(question_sets_path, quiz_data_path, false, ['A', 'G', 'I', 'Q', 'R', 'S', 'X', 'V', 'M'].to_vec(), ",".to_string(), "".to_string(), false)
}

pub fn qperf(question_sets_path: &str, quiz_data_path: &str, verbose: bool, types: Vec<char>, delim: String, tourn: String, display_rounds: bool) -> Result<(Vec<String>, String), Box<dyn std::error::Error>> {
    let mut warns = Vec::new();
    
    // Validate paths
    let (set_paths, data_paths) = validate_and_build_paths(question_sets_path, quiz_data_path, verbose)?;

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

    //map round number to question types
    let question_types_by_round = get_question_types_by_round(set_paths, verbose, &mut warns);

    //read quiz data file
    let (records, quizzer_names) = get_records(data_paths, verbose, tourn, &mut warns);
    
    let num_quizzers = quizzer_names.len();
    let num_question_types = QUESTION_TYPE_INDICES.len();

    let mut attempts: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut correct_answers: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut bonus_attempts: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];
    let mut bonus: Vec<Vec<u32>> = vec![vec![0; num_question_types]; num_quizzers];

    //updatable list of rounds, used track team scores.
    let mut rounds: Vec<Round> = Vec::new();
    if verbose {
        eprintln!("Beginning to process quiz data");
    }

    update_arrays(&mut warns, records, &quizzer_names, question_types_by_round, &mut attempts, &mut correct_answers, &mut bonus_attempts, &mut bonus, verbose, &mut rounds);

    let result = build_individual_results(quizzer_names, attempts, correct_answers, bonus_attempts, bonus, types, delim.clone());

    //append team results to result
    let team_result = build_team_results(&mut warns, rounds, delim.clone(), verbose, display_rounds);
    let result = format!("{}\n{}", result, team_result);

    Ok((warns, result))
}

fn validate_and_build_paths(question_sets_path: &str, quiz_data_path: &str, verbose: bool) -> Result<(Vec<std::path::PathBuf>, Vec<std::path::PathBuf>), Box<dyn std::error::Error>> {
    //Check if paths begin with "" or '' and remove them if they do.
    let question_sets_path = question_sets_path.trim_matches('\'').trim_matches('"');
    let quiz_data_path = quiz_data_path.trim_matches('\'').trim_matches('"');

    //Check if paths contain a comma. If they do, it's likely the user entered a comma separated list of question types.
    let set_paths_str: Vec<&str> = question_sets_path.split(',').collect();
    let data_paths_str: Vec<&str> = quiz_data_path.split(',').collect();
    
    //validate paths. data must be .csv, sets must be .rtf
    for path in &set_paths_str {
        if !Path::new(path).exists() {
            return Err(format!("Error: The path to the question sets does not exist: {}", path).into());
        }
        if Path::new(path).extension().unwrap() != "rtf" {
            return Err(format!("Error: The path to the question sets is not an RTF file: {}", path).into());
        }
    }
    for path in &data_paths_str {
        if !Path::new(path).exists() {
            return Err(format!("Error: The path to the quiz data does not exist: {}", path).into());
        }
        if Path::new(path).extension().unwrap() != "csv" {
            return Err(format!("Error: The path to the quiz data is not a CSV file: {}", path).into());
        }
    }

    //convert to PathBuf
    let set_paths: Vec<std::path::PathBuf> = set_paths_str.iter().map(|s| std::path::PathBuf::from(s)).collect();
    let data_paths: Vec<std::path::PathBuf> = data_paths_str.iter().map(|s| std::path::PathBuf::from(s)).collect();

    if verbose {
        eprintln!("Question Sets Paths: {:?}", set_paths);
        eprintln!("Quiz Data Paths: {:?}", data_paths);
    }

    Ok((set_paths, data_paths))
}

fn get_question_types_by_round(set_paths: Vec<PathBuf>, verbose: bool, warns: &mut Vec<String>) -> HashMap<String, Vec<char>> {
    let mut question_types_by_round: HashMap<String, Vec<char>> = HashMap::new();
    for entry in set_paths {
        if verbose {
            eprintln!("Found RTF file: {:?}", entry);
        }
        let question_types = read_rtf_file(entry.to_str().unwrap(), warns);
        //iterate through the map from this file and add to the main map, checking for duplicate round numbers and giving warnings for them.
        for (round_number, question_types) in question_types.unwrap() {
            if question_types_by_round.contains_key(&round_number) {
                eprintln!("Warning: Duplicate question set number: {}, using only the first.", round_number);
            } else {
                question_types_by_round.insert(round_number, question_types);
            }
        }
    }
    if verbose {
        eprintln!("{:?}", question_types_by_round);
    }

    question_types_by_round
}

fn get_records(data_paths: Vec<PathBuf>, verbose: bool, tourn: String, warns: &mut Vec<String>) -> (HashMap<String, RecordCollection>, Vec<(String, String)>) {
    let mut quiz_records = vec![];    
    for entry in data_paths {
        if verbose {
            eprintln!("Found CSV file: {:?}", entry);
        }
        //read quiz data file
        match read_csv_file(entry.to_str().unwrap()) {
            Ok(records) => {
                for record in records {
                    quiz_records.push(record);
                }
            }
            Err(e) => eprintln!("Quiz data contains formatting error: {}", e),
        }
    }

    let count_records = quiz_records.len();

    let filtered_records = filter_records(quiz_records, tourn.clone());

    if filtered_records.len() == 0 && count_records > 0 {
        warns.push(format!("Warning: No records found for tournament {}", tourn));
    }
    if verbose {
        eprintln!("Found {} records", filtered_records.len());
    }
    let (quizzer_names, records) = get_quizzer_names(filtered_records.clone(), verbose, warns);
    if verbose {
        eprintln!("Quizzer Names: {:?}", quizzer_names);
    }

    (records, quizzer_names)
}

fn build_individual_results(quizzer_names: Vec<(String, String)>, attempts: Vec<Vec<u32>>, correct_answers: Vec<Vec<u32>>, bonus_attempts: Vec<Vec<u32>>, bonus: Vec<Vec<u32>>, types: Vec<char>, delim: String) -> String {
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
    for (i, names) in quizzer_names.iter().enumerate() {
        //QuizMachine outputs often put single quotes around quizzer names. Check for them and remove them if present.
        let quizzer_name = names.0.trim_matches('\'');
        let team = names.1.trim_matches('\'');
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

fn update_arrays(warns: &mut Vec<String>, records: HashMap<String, RecordCollection>, quizzer_names: &Vec<(String, String)>, question_types: HashMap<String, Vec<char>>, attempts: &mut Vec<Vec<u32>>, correct_answers: &mut Vec<Vec<u32>>, bonus_attempts: &mut Vec<Vec<u32>>, bonus: &mut Vec<Vec<u32>>, verbose: bool, rounds: &mut Vec<Round>) {
    //list of skipped rounds
    let mut missing: Vec<String> = Vec::new();

    struct TeamStat {
        team_name: String,
        team_score: i32,
        active_quizzers: Vec<(String, u32, u32)>,//used to track when quizzers earn a team bonus or point deduction
        //String: quizzer name, u32: count questions (NOT BONUSES) correct, u32: count questions (NOT BONUESES) incorrect.

        /*
        BONUS RULES:
        1. If the opposing team gets a question wrong, this team gets a chance to attempt the question for a bonus (half points). +10pts
        2. If a third or fourth quizzer from this team answers a question correctly, they get a bonus. +10pts
        3. If any quizzer answers 4 questions correctly in a round without error, their team gets a bonus. +10pts
        4. If any quizzer answers 3 questions incorrectly in a round, their team gets a point deduction. -10pts
        5. Incorrect answers after question 16 are a deduction of 10 points.
         */
    }

    for round_name in records.keys() {
        if verbose {
            eprintln!("\nStarting next round: {}", round_name);
        }
        let mut teams: Vec<TeamStat> = Vec::new();
        let record_collection = records.get(round_name).unwrap();

        let mut round = Round {
            round_number: record_collection.round.clone(),
            room_number: record_collection.room.clone(),
            team_names: record_collection.teams.iter().map(|t| t.0.clone()).collect(),
            //initialize team scores to 0
            team_scores: vec![0; record_collection.teams.len()],
        };

        for team_name in round.team_names.iter() {
            teams.push(TeamStat {
                team_name: team_name.clone(),
                team_score: 0,
                active_quizzers: Vec::new(),
            });
        }

        for record in &record_collection.records {

            // Split the record by commas to get the columns
            let columns: Vec<&str> = record.into_iter().collect();
            // Get the event type code, quizzer name, and question number
            let event_code = columns.get(10).unwrap_or(&"");

            let team_number: usize = columns.get(8).unwrap_or(&"").parse().unwrap_or(0);

            let quizzer_name = columns.get(7).unwrap_or(&"");

            let question_number = columns.get(5).unwrap_or(&"").trim_matches('\'').parse::<usize>().unwrap_or(0) - 1;

            // Find the index of the quizzer in the quizzer_names array
            let quizzer_index = quizzer_names.iter().position(|n| n.0 == *quizzer_name).unwrap_or(0);

            // Check if the round is in the question types map
            let mut invalid_question_type = false;
            if !question_types.contains_key(&record_collection.round as &str) {
                if !missing.contains(&round.round_number.to_string()) {
                    missing.push(round.round_number.to_string());
                    //warns.push(format!("Missing question set for round {}! Ignoring question types for this round.", round.round_number));
                    invalid_question_type = true;
                }
                /*eprintln!("Warning: Skipping record due to missing question set for round {}", round.round_number);
                continue;*/
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
                eprint!("RNum: {} ", round.round_number);
            }
            if verbose {
                eprint!("QNum: {} ", question_number + 1);
            }
            // Get the question type based on question number
            let mut question_type = 'G';
            if invalid_question_type {
                question_type = '/';
            } else if question_types.contains_key(&record_collection.round as &str) && (question_number + 1) < question_types.get(&record_collection.round as &str).unwrap().len() {
                question_type = question_types.get(&record_collection.round as &str).unwrap_or(&vec!['G'])[question_number];
            }

            //Q, R, and V all count towards a total for memory verses.
            let memory = question_type == 'Q' || question_type == 'R' || question_type == 'V';
            if verbose {
                eprint!("QType: {} ", question_type);
            }
            // Find the index of the question type in the arrays
            let question_type_index = *QUESTION_TYPE_INDICES.get(&question_type).unwrap_or(&0);
            if verbose {
                eprintln!("QTInd: {} ", question_type_index);
            }
            // Update the arrays based on the event type code
            match *event_code {
                "'TC'" => {//Quizzer attempted to answer a question and got it right.
                    attempts[quizzer_index][question_type_index] += 1;
                    correct_answers[quizzer_index][question_type_index] += 1;
                    //also add for memory total
                    if memory {
                        attempts[quizzer_index][8] += 1;
                        correct_answers[quizzer_index][8] += 1;
                    }
                    /*Add 20 (full points) to team score. Add 1 question for the quizzer.
                    If this is the quizzer's 4th question without any incorrect, add 10 point bonus to team score.
                    If this quizzer is the 3rd or 4th to get a question right, add 10 point bonus to team score.*/
                    if let Some(team) = teams.get_mut(team_number) {
                        team.team_score += 20;
                        if verbose {
                            eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a question right. Added 20 points to team {}.", round.room_number, round.round_number, question_number + 1, quizzer_name, team.team_name);
                        }
                        //see if the quizzer is already in the list. Check for name only, not correct/incorrrect count.
                        if !team.active_quizzers.iter().any(|q| q.0 == *quizzer_name) {
                            team.active_quizzers.push((quizzer_name.to_string(), 1, 0));
                        } else {
                            let quizzer = team.active_quizzers.iter_mut().find(|q| q.0 == *quizzer_name).unwrap();
                            quizzer.1 += 1;
                            if quizzer.1 == 4 && quizzer.2 == 0 {
                                if verbose {
                                    eprintln!("[Team Scoring] Quiz-out bonus applied to team {}.", team.team_name);
                                }
                                team.team_score += 10;
                            }
                        }
                        //Check if at least 3 quizzers on this team have a .1 (second element of tuple, the u32) greater than 0
                        //AND that the current quizzer had a .1 exactly 1 (because this is their first correct question this round)
                        if team.active_quizzers.iter().filter(|q| q.1 > 0).count() >= 3 {
                            if let Some(quizzer) = team.active_quizzers.iter_mut().find(|q| q.0 == *quizzer_name) {
                                if quizzer.1 == 1 {
                                    team.team_score += 10;//Apply 3rd or 4th person bonus.
                                    if verbose {
                                        eprintln!("[Team Scoring] 3rd/4th person bonus applied to team {}.", team.team_name);
                                    }
                                }
                            }
                        }
                    } else {//This should NEVER happen. If it does, something is very wrong with the data.
                        if teams.len() <= team_number {
                            teams.push(TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            });
                        } else {
                            teams[team_number] = TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            };
                        }
                        warns.push(format!("Warning: Team number {} added mid-round in room {} round {}. This should not happen.", team_number, round.room_number, round.round_number));
                    }
                }
                "'TE'" => {//Quizzer attempted a question but got it wrong.
                    attempts[quizzer_index][question_type_index] += 1;
                    if memory {
                        attempts[quizzer_index][8] += 1;
                    }
                    //Deduct 10 points if EITHER we are on or after question 16, or this is the quizzer's 3rd incorrect answer.
                    //Incorrect answers are in .2, the third element of the tuple.
                    if let Some(team) = teams.get_mut(team_number) {
                        if let Some(quizzer) = team.active_quizzers.iter_mut().find(|q| q.0 == *quizzer_name) {
                            quizzer.2 += 1;
                            if quizzer.2 == 3 || question_number >= 15 {
                                team.team_score -= 10;
                                if verbose {
                                    eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a question wrong. Deducted 10 points from team {}.", round.room_number, round.round_number, question_number + 1, quizzer_name, team.team_name);
                                }
                            } else {
                                if verbose {
                                    eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a question wrong. No penalty applied.", round.room_number, round.round_number, question_number + 1, quizzer_name);
                                }
                            }
                        } else {
                            if question_number >= 15 {
                                team.team_score -= 10;
                                if verbose {
                                    eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a question wrong. Deducted 10 points from team {}.", round.room_number, round.round_number, question_number + 1, quizzer_name, team.team_name);
                                }
                            } else if verbose {
                                eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a question wrong. No penalty applied.", round.room_number, round.round_number, question_number + 1, quizzer_name);
                            }
                            let new_quizzer = (quizzer_name.to_string(), 0, 1);
                            team.active_quizzers.push(new_quizzer);
                        }
                    } else {
                        if teams.len() <= team_number {
                            teams.push(TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            });
                        } else {
                            teams[team_number] = TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            };
                        }
                        warns.push(format!("Warning: Team number {} added mid-round in room {} round {}. This should not happen.", team_number, round.room_number, round.round_number));
                    }
                }
                "'BC'" => {//Quizzer answered a bonus question correctly.
                    bonus_attempts[quizzer_index][question_type_index] += 1;
                    bonus[quizzer_index][question_type_index] += 1;
                    if memory {
                        bonus_attempts[quizzer_index][8] += 1;
                        bonus[quizzer_index][8] += 1;
                    }
                    //Add bonus of 10 to team score. Make sure the quizzer is considered active.
                    if let Some(team) = teams.get_mut(team_number) {
                        team.team_score += 10;
                        if verbose {
                            eprintln!("[Team Scoring] Rm: {} Rd: {} Q: {} Quizzer {} got a bonus right. Added 10 points to team {}.", round.room_number, round.round_number, question_number + 1, quizzer_name, team.team_name);
                        }
                    } else {
                        if teams.len() <= team_number {
                            teams.push(TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            });
                        } else {
                            teams[team_number] = TeamStat {
                                team_name: quizzer_name.to_string(),
                                team_score: 0,
                                active_quizzers: Vec::new(),
                            };
                        }
                        warns.push(format!("Warning: Team number {} added mid-round in room {} round {}. This should not happen.", team_number, round.room_number, round.round_number));
                    }
                    //If quizzer not in active list, add them.
                    if !teams.iter().any(|t| t.active_quizzers.iter().any(|q| q.0 == *quizzer_name)) {
                        if let Some(team) = teams.get_mut(team_number) {
                            team.active_quizzers.push((quizzer_name.to_string(), 0, 0));
                        }
                    }
                }
                "'BE'" => {//Quizzer answered a bonus question incorrectly.
                    bonus_attempts[quizzer_index][question_type_index] += 1;
                    if memory {
                        bonus_attempts[quizzer_index][8] += 1;
                    }
                    //This does nothing to team scoring. Move along.
                }
                "'TN'" => {//Team name. Use team name to see if it's already listed. If not, add it.
                    if teams.len() <= team_number {
                        teams.push(TeamStat {
                            team_name: quizzer_name.to_string(),
                            team_score: 0,
                            active_quizzers: Vec::new(),
                        });
                    } else {
                        teams[team_number] = TeamStat {
                            team_name: quizzer_name.to_string(),
                            team_score: 0,
                            active_quizzers: Vec::new(),
                        };
                    }
                }
                _ => {}
            }
            if verbose {
                //print state of current round for debugging.
                //round number, room number, question number, teams, scores.
                eprintln!("Current Round: {} Room: {} Question: {} Current Teams: {:?} Current Scores: {:?}", round.room_number, round.round_number, question_number + 1, 
                    teams.iter().map(|t| t.team_name.clone()).collect::<Vec<String>>(), teams.iter().map(|t| t.team_score).collect::<Vec<i32>>());
            }
        }

        round.team_scores = teams.iter().map(|t| t.team_score).collect();
        
        //Add the round to the list of rounds.
        rounds.push(round);
    }
    
    if missing.len() > 0 {
        //eprintln!("Warning: Some records were skipped due to missing question sets");
        warns.push("Warning: Some rounds are missing question sets! These questions will be treated as general!".to_string());
        //eprintln!("Skipped Rounds: {:?}", missing);
        warns.push(format!("Skipped Rounds: {:?}", missing));
        //Display the question set numbers found in the RTF files, sort them for easier reading.
        let mut found_rounds: Vec<_> = question_types.keys().collect();
        found_rounds.sort();
        eprintln!("Found Question Sets: {:?}", found_rounds);
        //eprintln!("If your question sets are not named correctly, please rename them to match the round numbers in the quiz data file");
        warns.push(format!("Round names must match between QuizMachine and the question set files!"));
    }
}

fn build_team_results(_warns: &mut Vec<String>, rounds: Vec<Round>, delim: String, verbose: bool, display_rounds: bool) -> String {
    let mut result = String::new();

    if verbose {
        eprintln!("Beginning to process {} rounds for team standing", rounds.len());
    }

    // This function will both display the results of each individual round (showing room number, round number, team names, and scores)
    // And it will also display the final ranking

    // Display the results of each individual round
    if display_rounds {
        result.push_str("Individual Round Results\n\n");
        for round in &rounds {
            result.push_str(&format!("Room: {}{} Round: {}\n", round.room_number, delim, round.round_number));
            for (i, team_name) in round.team_names.iter().enumerate() {
                result.push_str(&format!("{}{} {}\n", team_name, delim, round.team_scores[i]));
            }
            result.push('\n');
        }
        result.push('\n');
    }

    /*Construct final results
    TEMPORARY BASIC SOLUTION
    Simple algorithm for now, assign each team points based on the number of teams it defeats in a given round.
    Rounds have up to 3 teams, so a team can earn 0, 1, or 2 points per round.
    Once all rounds are processed, sort teams by number of points earned.*/
    /*let mut team_points: HashMap<String, u32> = HashMap::new();
    let mut team_totals: HashMap<String, u32> = HashMap::new();
    for round in &rounds {
        for (i, team_name) in round.team_names.iter().enumerate() {
            let team_score = round.team_scores[i];
            let mut points = 0;
            for (j, other_team_name) in round.team_names.iter().enumerate() {
                if i == j || other_team_name == "''" {//skip the team's own score and any empty team names.
                    continue;
                }
                if team_score > round.team_scores[j] {//if the team's score is higher than the other team's score, they get a point.
                    points += 1;
                }
            }
            let team_points = team_points.entry(team_name.clone()).or_insert(0);//get the team's points, or add them if they don't exist.
            *team_points += points;
            //Add this team's score from this round to their total.
            let team_total = team_totals.entry(team_name.clone()).or_insert(0);
            *team_total += team_score;
        }
    }

    //sort the teams by points
    let mut team_points_vec: Vec<_> = team_points.iter().collect();
    team_points_vec.sort_by(|a, b| b.1.cmp(a.1));//sort by points descending


    // Display the final ranking: 
    result.push_str("Final Ranking\n\n");

    // Header. Team names, total points, total score, separate with delim.
    result.push_str(&format!("Team{}Points{}Total Score\n", delim, delim));
    for (team_name, points) in team_points_vec {
        result.push_str(&format!("{}{}{}{}{}\n", team_name, delim, points, delim, team_totals.get(team_name).unwrap_or(&0)));
    }*/

    let rankings = rank_teams(rounds);

    result.push_str("Team Results\n\n");

    //Display brief explanation of how teams are ranked.
    result.push_str("Teams are ranked first by number of losses, then by number of wins, then by head-to-head record. \n\n");

    result.push_str(&format!("Name{}Placement{}Wins{}Losses{}Total Score\n", delim, delim, delim, delim));
    for ranking in rankings {
        let team_name = ranking.0.trim_matches('\'');
        result.push_str(&format!("{}{}{}{}{}{}{}{}{}\n", team_name, delim, ranking.1, delim, ranking.2, delim, ranking.3, delim, ranking.4));
    }

    result
}

struct RecordCollection {
    pub room: String,
    pub round: String,
    pub teams: Vec<(String, Vec<String>)>,
    pub records: Vec<csv::StringRecord>
}

fn get_quizzer_names(records: Vec<csv::StringRecord>, verbose: bool, warns: &mut Vec<String>) -> (Vec<(String, String)>, HashMap<String, RecordCollection>) {
    let mut round_teams: Vec<(String, Vec<String>)> = Vec::new();
    let mut round_string: String = "".to_string();
    let mut room_string: String = "".to_string();
    let mut confirmed_quizzers: Vec<(String, String)> = Vec::new();
    let mut confirmed_teams: Vec<String> = Vec::new();
    let mut action = false;

    let mut confirmed_records: HashMap<String, RecordCollection> = HashMap::new();
    let mut candidate_records: Vec<csv::StringRecord> = Vec::new();

    let mut match_string = String::new();

    for record in records {
        /*
        So there's a really dumb problem here.
        Sometimes, the output from QuizMachine includes leftover team names from practice sessions.
        While I've never seen actual questions from these practice sessions show up, I HAVE seen the names appear.
        This can mean a quizzer's name appears in two teams (one from practice, one from the actual quiz).
        The below code is an attempt to remove practice team and quizzer names by only adding teams when they participate in 'action'
        */

        if verbose {
            eprintln!("{:?}", record)
        }

        // Split the record by commas to get the columns
        let columns: Vec<&str> = record.into_iter().collect();
        let ecode = columns.get(10).unwrap_or(&"");//if this is "TN", it's a team name. If it's "QN", it's a quizzer name.
        let name = columns.get(7).unwrap_or(&"").to_string();//The name of either a quizzer or a team, depending on the event code.
        
        let team_number_str = columns.get(8).unwrap_or(&"").to_string();
        let team_number_string = team_number_str.trim_matches('\'');//team number from the current record. This gets reset to 0 at the start of each round.
        let team_number: usize = team_number_string.parse().unwrap_or(0);

        let seat_number_str = columns.get(9).unwrap_or(&"").to_string();
        let seat_number_string = seat_number_str.trim_end_matches('\'');
        let seat_number: usize = seat_number_string.parse().unwrap_or(0);
        
        if check_new_round(&round_string, &room_string, &columns) {//Indicates start of new round. Check if current teams can be confirmed.
            if check_valid_round(&mut round_teams, &mut confirmed_teams, &mut confirmed_quizzers, verbose, &mut action) {
                //remove teams with no name.
                round_teams.retain(|t| t.0 != "''" && t.0 != "");
                //remove quizzers with no name on each team
                for team in &mut round_teams {
                    team.1.retain(|q| q != "''" && q != "");
                }

                let round = RecordCollection {
                    room : room_string.clone(),
                    round : round_string.clone(),
                    teams : round_teams.clone(),
                    records : candidate_records.clone()
                };
                if confirmed_records.contains_key(&match_string) {
                    warns.push(format!("Warning: Duplicate round number: {}, overwriting!", match_string));
                    confirmed_records.remove(&match_string);
                }

                confirmed_records.insert(match_string.clone(), round);
                if verbose {//meant to be verbose
                    let mut team_names:Vec<String> = Vec::new();
                    for tn in round_teams.clone() {
                        team_names.push(tn.0);
                    }
                    eprintln!("Confirming round {} with teams {:?} and {} records", match_string, round_teams, candidate_records.len());
                }
            }
            candidate_records.clear();
            round_teams.clear();

            match_string = "Rm".to_string();
            match_string.push_str(columns.get(3).unwrap_or(&"").trim_matches('\''));
            match_string.push_str("Rd");
            match_string.push_str(columns.get(4).unwrap_or(&"").trim_matches('\''));

            room_string = columns.get(3).unwrap_or(&"").to_string();
            round_string = columns.get(4).unwrap_or(&"").to_string();
        }
        
        if ecode == &"'TN'" {//team name. Check if they're already in the map, and add them if not.
            //Insert at team_number, or replace current team_number if currently taken.
            while round_teams.len() <= team_number {
                round_teams.push(("".to_string(), Vec::new()));
            }
            round_teams[team_number] = (name.clone(), Vec::new());
            if verbose {
                eprintln!("Set team number {} to {}", team_number, name);
            }
        } else if ecode == &"'QN'" {//quizzer name. Add to the team's list.
            if round_teams.len() <= team_number {
                while round_teams.len() <= team_number {
                    round_teams.push(("".to_string(), Vec::new()));
                }
                round_teams[team_number] = ("".to_string(), Vec::new());
            }

            while round_teams[team_number].1.len() <= seat_number {
                round_teams[team_number].1.push("".to_string());
            }
            round_teams[team_number].1[seat_number] = name.clone();
            if verbose {
                eprintln!("Set seat number {} to {} for {}", seat_number, name, round_teams[team_number].0);
                eprint!("Current lineup: ");
                for team in &round_teams {
                    eprint!("{} {:?} ", team.0, team.1);
                }
                eprintln!();
            }
        } else if ecode == &"'BC'" || ecode == &"'BE'" || ecode == &"'TC'" || ecode == &"'TE'" {//action has happened, teams present in this round can be confirmed.
            action = true;
            candidate_records.push(record.clone());
            if verbose {
                eprintln!("Action happened during this round. It's probably not junk data.");
            }

            //Combine columns 3 and 4 for a unique round number.
        } 
    }
    //check last round
    if verbose {
        eprintln!("Checking last round, {} records remaining", candidate_records.len());
    }
    if check_valid_round(&mut round_teams, &mut confirmed_teams, &mut confirmed_quizzers, verbose, &mut action) {
        let round = RecordCollection {
            room : room_string.clone(),
            round : round_string.clone(),
            teams : round_teams.clone(),
            records : candidate_records.clone()
        };
        if confirmed_records.contains_key(&match_string) {
            warns.push(format!("Warning: Duplicate round number: {}, overwriting!", match_string));
            confirmed_records.remove(&match_string);
        }
        confirmed_records.insert(match_string.clone(), round);
        if verbose {
            let mut team_names:Vec<String> = Vec::new();
            for tn in round_teams.clone() {
                team_names.push(tn.0);
            }
            eprintln!("Confirming round {} with teams {:?} and {} records", match_string, round_teams, candidate_records.len());
        }
    }

    if verbose {
        eprintln!("Confirmed Teams: {:?}", confirmed_teams);
        eprintln!("Confirmed Quizzers: {:?}", confirmed_quizzers);
    }

    (confirmed_quizzers, confirmed_records)
}

fn check_new_round(round: &String, room: &String, columns: &Vec<&str>) -> bool {
    let mut new_round = false;
    if *round != columns.get(4).unwrap_or(&"").to_string() || *room != columns.get(3).unwrap_or(&"").to_string() {
        new_round = true;
    }
    new_round
}

fn check_valid_round(round_teams: &mut Vec<(String, Vec<String>)>, confirmed_teams: &mut Vec<String>, confirmed_quizzers: &mut Vec<(String, String)>, verbose: bool, action: &mut bool) -> bool {
    let mut valid = false;
    if *action {
        for i in 0..round_teams.len() {
            if !confirmed_teams.contains(&round_teams[i].0) {
                confirmed_teams.push(round_teams[i].0.clone());
            }
            for j in 0..round_teams[i].1.len() {
                if !confirmed_quizzers.iter().any(|(quizzer, _)| quizzer == &round_teams[i].1[j]) && round_teams[i].1[j] != "''" && round_teams[i].1[j] != "" {//don't add empty strings or blank names.
                    confirmed_quizzers.push((round_teams[i].1[j].clone(), round_teams[i].0.clone()));
                }
            }
        }

        if verbose {
            eprintln!("Confirming Teams: {:?}", round_teams);
        }
        valid = true;
    } else {
        if verbose {
            eprintln!("No action taken in round, teams: {:?} might be from practice", round_teams);
        }
    }
    *action = false;
    valid
}

fn filter_records(records: Vec<csv::StringRecord>, tourn: String) -> Vec<csv::StringRecord> {
    let mut filtered_records = Vec::new();
    let event_codes = vec!["'TC'", "'TE'", "'BC'", "'BE'", "'TN'", "'QN'", "'RM'"]; // event type codes

    for record in records {
        // Split the record by commas to get the columns
        let columns: Vec<&str> = record.into_iter().collect();

        //skip rounds with different tournament name
        if tourn != "" && tourn != *columns.get(1).unwrap() {
            continue;
        }

        // Check if the 11th column contains the event type codes
        if columns.get(10).map_or(false, |v| event_codes.contains(&v)) {
            filtered_records.push(csv::StringRecord::from(columns));
        }
    }

    filtered_records
}

fn read_rtf_file(path: &str, warns: &mut Vec<String>) -> io::Result<HashMap<String, Vec<char>>> {
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

    //Check if any of the rounds have a weird name, such as an empty string. This would indicate the RTF file is not formatted correctly.
    for (round, _types) in &question_types_by_round {
        if round == "''" {
            warns.push("Warning: RTF question set file might have been formatted incorrectly. Please use only the original RTF files!".to_string());
        }
    }

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

//Function to generate a unique hash for a team name
fn hash_string(string: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    string.hash(&mut hasher);
    hasher.finish()
}

//Function to generate a unique key for any pair of two teams regardless of order.
fn generate_matchup_key(team_a: &str, team_b: &str) -> (u64, String) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut teams = [team_a, team_b];
    teams.sort();
    let mut hasher = DefaultHasher::new();
    teams[0].hash(&mut hasher);
    teams[1].hash(&mut hasher);
    (hasher.finish(), teams[0].to_string())
}


pub fn rank_teams(rounds: Vec<Round>) -> Vec<(String, u32, u32, u32, i32)> {
    let mut wins: HashMap<String, u32> = HashMap::new();
    let mut losses: HashMap<String, u32> = HashMap::new();
    let mut total_scores: HashMap<String, i32> = HashMap::new();
    let mut head_to_head: HashMap<u64, (i32, i32)> = HashMap::new(); // Uses unique key for matchups
    
    let mut teams: HashSet<String> = HashSet::new();
    
    // Initialize team records
    for round in &rounds {
        for (team, &score) in round.team_names.iter().zip(&round.team_scores) {
            if !team.is_empty() {
                teams.insert(team.clone());
                wins.entry(team.clone()).or_insert(0);
                losses.entry(team.clone()).or_insert(0);
                total_scores.entry(team.clone()).or_insert(0);
                *total_scores.get_mut(team).unwrap() += score;
            }
        }
    }
    
    // Process match results
    for round in &rounds {
        let mut scored_teams: Vec<(String, i32)> = round
            .team_names.iter()
            .cloned()
            .zip(round.team_scores.iter().cloned())
            .filter(|(team, _)| !team.is_empty())
            .collect();
        
        if scored_teams.len() < 2 {
            continue;
        }
        
        // Sort teams by score in descending order
        scored_teams.sort_by(|a, b| b.1.cmp(&a.1));
        //Add wins and losses for each team. 1 win for each team that scored less than the current team, 1 loss for each team that scored more.
        for (team, score) in scored_teams.iter() {//skip the first team, since they're the winner.
            for (_other_team, other_score) in scored_teams.iter() {
                if score > other_score {
                    *wins.get_mut(team).unwrap() += 1;
                } else if score < other_score {//ties are not counted as wins or losses.
                    *losses.get_mut(team).unwrap() += 1;
                }
                //No need to skip self, since the score will always be equal.
            }
        }

        //Check if this is a two-team round or a three-team round. This will affect how we handle head-to-head scoring.
        if scored_teams.len() == 2 {
            //In a two-team round, the two teams play each other. This is a head-to-head matchup.
            handle_matchup(&mut head_to_head, &scored_teams[0].0, &scored_teams[1].0, scored_teams[0].1, scored_teams[1].1);
        } else if scored_teams.len() == 3 {
            //In a three-team round, each team plays the other two teams. That's effectively 3 different head-to-heads at the same time.
            //To demonstrate: A vs B, A vs C, B vs C. Each of these matchups is treated as a separate head-to-head.
            handle_matchup(&mut head_to_head, &scored_teams[0].0, &scored_teams[1].0, scored_teams[0].1, scored_teams[1].1);
            handle_matchup(&mut head_to_head, &scored_teams[0].0, &scored_teams[2].0, scored_teams[0].1, scored_teams[2].1);
            handle_matchup(&mut head_to_head, &scored_teams[1].0, &scored_teams[2].0, scored_teams[1].1, scored_teams[2].1);
        }
    }
    
    // Convert to a sortable vector
    let mut ranking: Vec<(String, u32, u32, u32, i32)> = teams.into_iter()
        .map(|team| (team.clone(), 0, wins[&team], losses[&team], total_scores[&team]))
        .collect();
    
    // Sorting logic: losses ASC, wins DESC, head-to-head points as tie-breaker
    ranking.sort_by(|a, b| {
        let loss_cmp = a.3.cmp(&b.3);
        if loss_cmp != std::cmp::Ordering::Equal {
            return loss_cmp;
        }
        
        let win_cmp = b.2.cmp(&a.2);
        if win_cmp != std::cmp::Ordering::Equal {
            return win_cmp;
        }
        
        let (key, lower_hash) = generate_matchup_key(&a.0, &b.0);
        //Ordering in the tuple is dependent on name hashing, not team score. Thus, team_a might actually be the second team in the tuple!
        //This is to ensure consistent ordering if the teams have multiple head-to-head matchups with each other.
        //Use lower_hash to determine which team is listed first in the tuple.
        let (head_to_head_a, head_to_head_b);
        if a.0 == lower_hash {//team a is the first team in the tuple.
            head_to_head_a = head_to_head.get(&key).map(|(a, _)| *a).unwrap_or(0);
            head_to_head_b = head_to_head.get(&key).map(|(_, b)| *b).unwrap_or(0);
        } else {//team b is the first team in the tuple.
            head_to_head_a = head_to_head.get(&key).map(|(_, a)| *a).unwrap_or(0);
            head_to_head_b = head_to_head.get(&key).map(|(b, _)| *b).unwrap_or(0);
        }
        head_to_head_b.cmp(&head_to_head_a)
    });
    
    // Assign placement rankings
    for (i, entry) in ranking.iter_mut().enumerate() {
        entry.1 = (i + 1) as u32;
    }
    
    ranking
}

fn handle_matchup(head_to_head: &mut HashMap<u64, (i32, i32)>, team_a: &str, team_b: &str, score_a: i32, score_b: i32) {
    let (key, lower_hash) = generate_matchup_key(team_a, team_b);
    //ordering in the tuple is dependent on the hash of team names, NOT the score! This is to ensure consistent ordering if the teams have multiple head-to-head matchups with each other.
    //Whichever team matches lower_hash is the team that will be listed first in the tuple.
    let (a_score, b_score) = head_to_head.entry(key).or_insert((0, 0));
    if lower_hash == team_a {
        *a_score += score_a;
        *b_score += score_b;
    } else {
        *a_score += score_b;
        *b_score += score_a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test for 'read_rtf_file' function
    #[test]
    fn test_read_rtf_file() {
        let sample_rtf_path = "tests/questions/sets.rtf"; // Ensure a sample file exists in `tests/`
        let result = read_rtf_file(sample_rtf_path, &mut Vec::new());
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
        let filtered = filter_records(read_csv_file("tests/quiz_data.csv").unwrap(), "".to_string());
        let expected = read_csv_file("tests/filtered_quiz_data.csv").unwrap();
        // Validate filtering logic (replace with actual expectations)
        assert_eq!(filtered.len(), expected.len());
    }
}