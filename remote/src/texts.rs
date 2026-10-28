//! Texts and the player language.

group! {
    /// Texts and the player language.
    "text": TextRequest, TextAnswer;

    /// The player language: the language of the framework's texts and every
    /// mod's in this session.
    anywhere Language "language" {} -> String, "its Paradox name, such as english or simp_chinese";
}
