use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process,
};

use tempfile::NamedTempFile;

use crate::{
    error::InquireResult,
    formatter::StringFormatter,
    prompts::prompt::{ActionResult, Prompt},
    ui::EditorBackend,
    validator::{ErrorMessage, StringValidator, Validation},
    Editor, InquireError,
};

use super::{action::EditorPromptAction, config::EditorConfig};

pub struct EditorPrompt<'a> {
    message: &'a str,
    config: EditorConfig<'a>,
    help_message: Option<&'a str>,
    formatter: StringFormatter<'a>,
    validators: Vec<Box<dyn StringValidator>>,
    error: Option<ErrorMessage>,
    tmp_file: NamedTempFile,
}

impl<'a> From<&'a str> for Editor<'a> {
    fn from(val: &'a str) -> Self {
        Editor::new(val)
    }
}

impl<'a> EditorPrompt<'a> {
    pub fn new(so: Editor<'a>) -> InquireResult<Self> {
        Ok(Self {
            message: so.message,
            config: (&so).into(),
            help_message: so.help_message,
            formatter: so.formatter,
            validators: so.validators,
            error: None,
            tmp_file: Self::create_file(so.file_extension, so.predefined_text)?,
        })
    }

    fn create_file(
        file_extension: &str,
        predefined_text: Option<&str>,
    ) -> std::io::Result<NamedTempFile> {
        let mut tmp_file = tempfile::Builder::new()
            .prefix("tmp-")
            .suffix(file_extension)
            .rand_bytes(10)
            .tempfile()?;

        if let Some(predefined_text) = predefined_text {
            tmp_file.write_all(predefined_text.as_bytes())?;
            tmp_file.flush()?;
        }

        Ok(tmp_file)
    }

    fn run_editor(&mut self) -> InquireResult<()> {
        process::Command::new(self.config.editor_command)
            .args(self.config.editor_command_args)
            .arg(self.tmp_file.path())
            .spawn()?
            .wait()?;

        Ok(())
    }

    fn validate_current_answer(&self) -> InquireResult<Validation> {
        let cur_answer = self.cur_answer()?;
        for validator in &self.validators {
            match validator.validate(&cur_answer) {
                Ok(Validation::Valid) => {}
                Ok(Validation::Invalid(msg)) => return Ok(Validation::Invalid(msg)),
                Err(err) => return Err(InquireError::Custom(err)),
            }
        }

        Ok(Validation::Valid)
    }

    fn cur_answer(&self) -> InquireResult<String> {
        let mut read_handler = fs::File::open(self.tmp_file.path())?;
        let mut submission = String::new();
        read_handler.read_to_string(&mut submission)?;

        let len = submission.trim_end_matches(&['\n', '\r'][..]).len();
        submission.truncate(len);

        Ok(submission)
    }
}

impl<'a, B> Prompt<B, EditorConfig<'a>, EditorPromptAction, String> for EditorPrompt<'a>
where
    B: EditorBackend,
{
    fn message(&self) -> &str {
        self.message
    }

    fn config(&self) -> &EditorConfig<'a> {
        &self.config
    }

    fn format_answer(&self, answer: &String) -> String {
        (self.formatter)(answer)
    }

    fn submit(&mut self) -> InquireResult<Option<String>> {
        let answer = match self.validate_current_answer()? {
            Validation::Valid => Some(self.cur_answer()?),
            Validation::Invalid(msg) => {
                self.error = Some(msg);
                None
            }
        };

        Ok(answer)
    }

    fn handle(&mut self, action: EditorPromptAction) -> InquireResult<ActionResult> {
        match action {
            EditorPromptAction::OpenEditor => {
                self.run_editor()?;
                Ok(ActionResult::NeedsRedraw)
            }
        }
    }

    fn render(&self, backend: &mut B) -> InquireResult<()> {
        let prompt = &self.message;

        if let Some(err) = &self.error {
            backend.render_error_message(err)?;
        }

        let path = Path::new(self.config.editor_command);
        let editor_name = path
            .file_stem()
            .and_then(|f| f.to_str())
            .unwrap_or("editor");

        backend.render_prompt(prompt, editor_name)?;

        if let Some(message) = self.help_message {
            backend.render_help_message(message)?;
        }

        Ok(())
    }
}
