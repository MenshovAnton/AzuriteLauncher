import { invoke } from "@tauri-apps/api/core";
import {useEffect, useState} from "react";

function App() {
    const [versions, setVersions] = useState<string[]>([]);
    const [selectedVersion, setSelected] = useState("");
    const [jvmPath, setJVMPath] = useState("");
    const [username, setUsername] = useState("");

    useEffect(() => {
        invoke<string[]>("get_versions").then(setVersions);
    })

    async function play() {
        await invoke<string>("start", { 
            jvmPath: jvmPath,
            gameVersion: selectedVersion, 
            username: username 
        });
    }

    return (
        <div>
            <form className={"params"}>
                <label htmlFor="versionSelector">
                    Version:
                    <select onChange={(e) => setSelected(e.target.value)}>
                        {versions.map(v => (
                            <option key={v} value={v}>{v}</option>
                        ))}
                    </select>
                </label>

                <label htmlFor="javaDirectory">
                    JVM path:
                    <input
                        value={jvmPath}
                        onChange={(e) => setJVMPath(e.target.value)}
                        type="text"
                        id="javaDirectory"
                        name="ver"
                        required
                    />
                </label>

                <label htmlFor="username">
                    Username:
                    <input
                        value={username}
                        onChange={(e) => setUsername(e.target.value)}
                        type="text"
                        id="username"
                        name="ver"
                        required
                    />
                </label>
            </form>

            <button onClick={play}>Play</button>
        </div>
    );
}

export default App;