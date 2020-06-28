import "core-js/stable";
import "regenerator-runtime/runtime";

import { Exocore, exocore } from "exocore";
import React from 'react';
import ReactDOM from 'react-dom';
import List from './list.js';

class App extends React.Component {
    constructor(props) {
        super(props);

        this.state = {
            status: 'disconnected',
            config: null,
        };

        const configJson = localStorage.getItem('config');
        if (!!configJson) {
            this.state.config = JSON.parse(configJson);
            this.connect(this.state.config);
        }
    }

    render() {
        if (!this.state.config) {
            return <ConfigInput onSet={this.setConfig.bind(this)} />;
        }

        if (this.state.status === 'connected') {
            return (<div>
                <button onClick={this.disconnect.bind(this)}>Reset</button>

                <List />
            </div>);
        } else {
            return this.renderLoading();
        }
    }

    renderLoading() {
        return (<div>
            <h3>Connecting...</h3>

            <button onClick={this.disconnect.bind(this)}>Reset</button>
        </div>);
    }

    disconnect() {
        this.setState({ config: null });
        localStorage.clear();
    }

    setConfig(configJson) {
        let config = JSON.parse(configJson);
        localStorage.setItem('config', configJson);
        this.setState({
            config: config,
        });

        this.connect(config);
    }

    connect(config) {
        Exocore.initialize(config).then((instance) => {
            Exocore.registry.registerMessage(exocore.test.TestMessage, 'exocore.test.TestMessage');
            Exocore.registry.registerMessage(exocore.test.TestMessage2, 'exocore.test.TestMessage2');

            instance.onChange = () => {
                this.setState({ status: Exocore.defaultInstance.status });
            }
        });
    }
}

class ConfigInput extends React.Component {
    constructor(props) {
        super(props);

        this.state = {
            text: ''
        }
    }

    render() {
        const textStyle = {
            width: 500 + 'px',
            height: 300 + 'px',
        };

        return (
            <div>
                <h3>Paste JSON node config</h3>
                <div><textarea value={this.state.text} onChange={this.onTextChange.bind(this)} style={textStyle} /></div>
                <button onClick={this.onAddClick.bind(this)}>Save</button>
            </div>
        )
    }

    onTextChange(e) {
        this.setState({
            text: e.target.value
        });
    }

    onAddClick(e) {
        this.props.onSet(this.state.text);
        this.setState({
            text: ''
        });
    }
}


ReactDOM.render(
    <App />,
    document.getElementById('root')
);

