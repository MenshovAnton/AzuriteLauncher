import { invoke } from "@tauri-apps/api/core";

function App() {

    async function play() {
        await invoke<string>("start", { dir: getInputValue("mc"), ver: getInputValue("ver")});
    }

    return (
        <div>
            <form >
                <label htmlFor="ver">
                    Версия:
                    <input
                        type="text"
                        id="ver"
                        name="ver"
                        placeholder={"1.20.1"}
                        required
                    />
                </label>

                <label htmlFor="mc">
                    Папка Minecraft:
                    <input
                        type="text"
                        id="mc"
                        name="ver"
                        placeholder={"X:/.minecraft"}
                        required
                    />
                </label>
            </form>

            <button onClick={play}>Играть</button>
        </div>
    );
}

function getInputValue(element: string) : string {
    let inputValue = document.getElementById(element) as HTMLInputElement;
    return inputValue.value;
}

export default App;