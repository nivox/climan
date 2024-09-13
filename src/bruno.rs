use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, line_ending, multispace0, space0},
    combinator::{map, opt},
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};

// Structure Definitions

#[derive(Debug, PartialEq)]
pub enum Block {
    DictionaryBlock(DictionaryBlock),
    TextBlock(TextBlock),
    ArrayBlock(ArrayBlock),
}

#[derive(Debug, PartialEq)]
pub struct DictionaryBlock {
    pub name: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Debug, PartialEq)]
pub struct TextBlock {
    pub name: String,
    pub content: String,
}

#[derive(Debug, PartialEq)]
pub struct ArrayBlock {
    pub name: String,
    pub entries: Vec<ArrayEntry>,
}

#[derive(Debug, PartialEq)]
pub struct ArrayEntry {
    pub value: String,
    pub enabled: bool,
}

// Utility functions

// Parse an identifier (block name or key)
fn identifier(input: &str) -> IResult<&str, String> {
    let res = parse_identifier(input);
    match res {
        Ok((input, s)) => Ok((input, s.to_string())),
        Err(e) => Err(e),
    }
}

// Parse a dictionary block entry (key-value pair)
fn dictionary_entry(input: &str) -> IResult<&str, Entry> {
    map(
        tuple((
            preceded(multispace0, opt(tag("~"))),
            preceded(space0, identifier),
            preceded(space0, char(':')),
            opt(is_not("\n")),
        )),
        |(disabled, key, _, value): (Option<&str>, String, char, Option<&str>)| Entry {
            key,
            value: value.unwrap_or("").to_string(),
            enabled: disabled.is_none(),
        },
    )(input)
}

// Parse a dictionary block
fn dictionary_block(input: &str) -> IResult<&str, DictionaryBlock> {
    map(
        tuple((
            preceded(multispace0, identifier),
            delimited(
                preceded(multispace0, char('{')),
                many0(terminated(dictionary_entry, line_ending)),
                preceded(multispace0, char('}')),
            ),
        )),
        |(name, entries): (String, Vec<Entry>)| DictionaryBlock { name, entries },
    )(input)
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    let mut idx = 0;
    let mut colon_idx = 0;

    for (i, c) in input.chars().enumerate() {
        match c {
            ':' => {
                colon_idx = i;
                idx = i;
            }
            ' ' | '\t' | '\n' => {
                if colon_idx == idx && idx > 0 {
                    idx = idx - 1;
                };
                break;
            }
            _ => idx = i,
        }
    }

    if idx > 0 {
        Ok((&input[idx + 1..], &input[..idx + 1]))
    } else {
        let error = nom::error::Error::new(input, nom::error::ErrorKind::Char);
        Err(nom::Err::Error(error))
    }
}

// Helper function to parse balanced braces
fn parse_braces(input: &str) -> IResult<&str, &str> {
    let mut depth = 0;
    let mut idx = 0;

    for (i, c) in input.chars().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    idx = i + 1; // Include the closing brace
                    break;
                }
            }
            _ => {}
        }
    }

    if depth == 0 {
        Ok((&input[idx..], &input[..idx]))
    } else {
        // Return an error if braces are unbalanced
        let error = nom::error::Error::new(input, nom::error::ErrorKind::Char);
        Err(nom::Err::Error(error))
    }
}

// Parse a text block
fn text_block(input: &str) -> IResult<&str, TextBlock> {
    map(
        tuple((
            preceded(multispace0, identifier),
            delimited(
                preceded(multispace0, char('{')),
                delimited(multispace0, parse_braces, multispace0),
                preceded(multispace0, char('}')),
            ),
        )),
        |(name, content): (String, &str)| TextBlock {
            name,
            content: content.trim().to_string(),
        },
    )(input)
}

// Parse an array entry
fn array_entry(input: &str) -> IResult<&str, ArrayEntry> {
    map(
        tuple((preceded(multispace0, opt(tag("~"))), identifier)),
        |(disabled, value): (Option<&str>, String)| ArrayEntry {
            value,
            enabled: disabled.is_none(),
        },
    )(input)
}

// Parse an array block
fn array_block(input: &str) -> IResult<&str, ArrayBlock> {
    map(
        tuple((
            preceded(multispace0, identifier),
            delimited(
                preceded(multispace0, char('[')),
                separated_list0(char('\n'), array_entry),
                preceded(multispace0, char(']')),
            ),
        )),
        |(name, entries): (String, Vec<ArrayEntry>)| ArrayBlock { name, entries },
    )(input)
}

// Parse any block (dictionary, text, or array)
fn parse_block(input: &str) -> IResult<&str, Block> {
    alt((
        map(dictionary_block, Block::DictionaryBlock),
        map(text_block, Block::TextBlock),
        map(array_block, Block::ArrayBlock),
    ))(input)
}

// Parse the entire input file
fn parse_bru(input: &str) -> IResult<&str, Vec<Block>> {
    many0(terminated(parse_block, multispace0))(input)
}

#[cfg(test)]
mod tests {
    use crate::bruno::{array_block, array_entry, dictionary_block, identifier, parse_bru};

    #[test]
    fn test_bru_parsing() {
        let result = parse_bru(
            r#"
            meta {
              name: Query api-clients
              type: http
              seq: 102
            }
            
            get {
              url: {{apiUrl}}/{{project-key}}/api-clients
              body: none
              auth: inherit
            }
            
            
            query {
              ~expand:
              ~sort: b
              ~limit: 10 
              ~offset: c
              ~withTotal: d 
              ~where: e 
              ~/^var[.][a-zA-Z0-9]+$/: f
            }
            
            script:post-response {
              var data = res.body;
              if(res.status == 200 || res.status == 201) {
                  if(data.results && data.results[0] && data.results[0].id && data.results[0].version){
                      bru.setEnvVar("api-client-id", data.results[0].id); 
                      bru.setEnvVar("api-client-version", data.results[0].version);
                  }
                  if(data.results && data.results[0] && data.results[0].key){
                      bru.setEnvVar("api-client-key", data.results[0].key); 
                  }
                  if(data.version){
                      bru.setEnvVar("api-client-version", data.version);
                  }
                  if(data.id){
                      bru.setEnvVar("api-client-id", data.id); 
                  }
                  if(data.key){
                      bru.setEnvVar("api-client-key", data.key);
                  }
            
              }
            }
            
            assert {
              res.status: in [200, 201]
            }
            "#,
        );
        assert_eq!(result.expect("Failed to parse bruno file").0, "");
    }

    #[test]
    fn test_bru_dict() {
        let result = dictionary_block(
            r#"dictionary {
                key1: value1
                key2: value2
                ~key3: value3
            }"#,
        );
        assert_eq!(result.expect("Failed to parse dictionary block").0, "");
    }

    #[test]
    fn test_bru_array() {
        let result = array_block(
            r#"array [
                value1
                value2
                ~value3
            ]"#,
        );
        assert_eq!(result.expect("Failed to parse array block").0, "");
    }

    #[test]
    fn test_bru_identifier() {
        let result = identifier(r#"test: other"#);
        assert_eq!(result.expect("Failed to parse array block").0, ": other");

        let result = identifier(r#"test:more other"#);
        assert_eq!(result.expect("Failed to parse array block").0, " other");
    }
}
