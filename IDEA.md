I am building an application to categorize my hard drive with movies. Today I struggle with organization; the movies are always messy and I can never find what I need.

Let's develop an application together that does the categorization automatically for me.

1. Files and Metadata

To facilitate organization, each movie will be inside a folder containing the Title of the work in Brazilian Portuguese and, in brackets, the ID of the work. Most movies today are not organized this way, so the program should identify movies in this situation, move them to a folder, and allow linking them to an existing work on the TMDB API.
In addition to the file, the system must maintain 1 .sqlite file file that must be saved on the Hard Drive choseen by the user, the TMDB ID, Title, and Genres.

2.

2. Folder Organization

Folders should be organized by genre as follows:

Movies/
- Action
- Comedy
- Animation

3. Interface

The interface should be simple and minimalist.

It must have 2 main panels:

Left Panel:
- The first item will be "Uncatalogued movies"
- The remaining items will be the registered genres
    - When selecting one, it will display the movies within the current category
    - When selecting a movie, it must be displayed in the right panel

Right Panel:

It must contain:
- Cover
- Name
- TMDB ID
- Movie Runtime
- Primary and secondary genres
- It must allow to:
    - Rename the movie
    - "Link" the movie
        - The Link action will change the TMDB ID in the database
        - It must also change the folder name to contain the [IMDB_ID]
        - Ensure that the TMDB ID is always at the end of the folder name
        - The movie file must have the same name as the folder
        - The poster must be saved in a 'poster' folder located inside the Hard Disk chosen
            - The database should store the path and the URL (fallback if fails to load file)
- Search the TMDB API and link it

The search action must display the following data:
- Name
- Year
- Genres
- Poster
- There must be:
    - A button to confirm changes

In addition to the panels, we also have a settings button; it will allow to:

- Translate the names of the genres
    - The information must be saved in the Database
    - The corresponding genre folder must be renamed to the name the user chose
    - The UI should always show the translated name
    - Genres can have the same name, they should be visualizy merged in the UI and use the same folder

4. Stack

Please use Rust, Tauri, React, Tailwind CSS, and TypeScript.
Create a localization file to facilitate the project's translation.
Make atomic commits whenever necessary.

5. Possible ambiguities

- If the movie has multiple genres, what should we do?
    - It will always be placed in a folder for its primary genre. But in the SQLite, it should store multiple genres and have a flag to point out what is the primary one.
- Will the TMDB API KEY be provided by a configuration file?
    - The TMDB API KEY will be provided from a .env file.
- Which folder path should we use?
    - The user should select the folder when launching the application. It should store it and not ask again when launched, unless an error occurs while opening the specified path.
- How can we detect if the movie is Uncatalogued?
    - If the video file is not inside a folder formatted as 'Title [TMDB]', it's Uncatalogued; you should also check if all folders exist inside our database.
- Where should the DB be stored?
    - The DB should be stored in the root of the HD as a sorta.db file.

6. Final

I need you to make a comprehensive plan (and write it down as PLAN.md) before coding anything, and make sure you ask me if I left anything unexplained or ambiguous.

Do atomic commits for every feature being maded.
Also start by creating the unit tests using a Test Driven Development (TDD) approach.